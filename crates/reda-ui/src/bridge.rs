//! The app's end of the bridge (ADR-0007, architecture.md#cli-and-control-port): it hosts
//! the control port so the CLI and MCP agents edit the open Project in this window.
//!
//! - The port is on unless the `agents.bridge` Preference turns it off; the status bar
//!   shows it and the agents connected.
//! - An agent's first connection waits on "Allow this agent?"; allowed agents are kept in
//!   `state.json` and let in without asking after that.
//! - Bridged Commands queue to the window's single writer and apply between frames,
//!   through [`Shell::run_bridged`]: labelled with their source in undo, never saved, and
//!   cancelling a Tool preview whose objects they change. They never wait for the user.
//! - Another `reda-app` asked by the OS to open a file of the open Project hands it over
//!   ([`hand_off`]) and exits; this window focuses the file's tab.
//!
//! Nothing here draws; the window calls [`Bridge::pump`] once per frame and shows
//! [`Bridge::asking`].

use std::collections::VecDeque;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use reda_control::{Admission, Agent, Call, ControlServer, client, protocol};
use reda_engine::command::{CommandError, UserDirs};
use reda_engine::journal::Source;
use reda_engine::model::DocumentKind;
use reda_engine::{preferences, state};
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::tr;

/// The Preference that switches the bridge.
const SWITCH: &str = "agents.bridge";

type Wake = Arc<dyn Fn() + Send + Sync>;

/// The control port as the window hosts it. Dropping it stops the port.
#[derive(Default)]
pub struct Bridge {
    running: Option<Running>,
    /// Agents waiting for the user to allow or deny them, oldest first.
    asking: VecDeque<Admission>,
    /// A file hand-off asked the window to come to the front.
    focus: bool,
    /// Why the port could not start.
    error: Option<String>,
    /// Wakes the window when a Call or an agent arrives, so it pumps without input.
    wake: Arc<Mutex<Option<Wake>>>,
}

struct Running {
    server: ControlServer,
    calls: Receiver<Call>,
    admissions: Receiver<Admission>,
    forwarders: Vec<JoinHandle<()>>,
}

impl Bridge {
    /// A bridge that is off.
    pub fn off() -> Bridge {
        Bridge::default()
    }

    /// Starts the port for `shell`'s engine unless the Preference turns it off. Without
    /// per-user folders there is nowhere to publish it, so it stays off.
    pub fn start(shell: &Shell) -> Bridge {
        let mut bridge = Bridge::off();
        if preferences::current(shell.engine().user_dirs())
            .agents
            .bridge
        {
            bridge.open(shell);
        }
        bridge
    }

    /// Whether the port is open.
    pub fn is_on(&self) -> bool {
        self.running.is_some()
    }

    /// Why the port failed to open, if it did.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Lets the bridge wake the window when work arrives.
    pub fn set_waker(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        *self.wake.lock().expect("not poisoned") = Some(Arc::new(wake));
    }

    /// Turns the bridge on or off and keeps the choice as the Preference.
    pub fn set_enabled(&mut self, shell: &mut Shell, on: bool) {
        if shell.engine().user_dirs().is_some() {
            let _ =
                shell.run_reporting("app.preferences.set", json!({ "key": SWITCH, "value": on }));
        }
        match on {
            true if !self.is_on() => self.open(shell),
            false => self.stop(),
            true => {}
        }
    }

    /// The agents connected now.
    pub fn agents(&self) -> Vec<Agent> {
        self.running
            .as_ref()
            .map(|r| r.server.agents())
            .unwrap_or_default()
    }

    /// The agents the user has allowed, from `state.json`.
    pub fn allowed(&self, shell: &Shell) -> Vec<String> {
        shell
            .engine()
            .user_dirs()
            .map(|dirs| state::read(dirs).allowed_agents)
            .unwrap_or_default()
    }

    /// Forgets an allowed agent: it is asked about again on its next connection.
    pub fn forget_agent(&mut self, shell: &mut Shell, name: &str) {
        let Some(dirs) = shell.engine().user_dirs() else {
            return;
        };
        if let Err(e) = state::update(dirs, |s| s.allowed_agents.retain(|a| a != name)) {
            shell.report(tr!("error-state-file", error = e.to_string()));
        }
    }

    /// The agent waiting for "Allow this agent?", if any.
    pub fn asking(&self) -> Option<&Agent> {
        self.asking.front().map(|a| &a.agent)
    }

    /// Answers "Allow this agent?" for the agent [`Bridge::asking`] shows. An allowed
    /// agent is remembered in `state.json`.
    pub fn answer(&mut self, shell: &mut Shell, allow: bool) {
        let Some(admission) = self.asking.pop_front() else {
            return;
        };
        if !allow {
            admission.deny();
            return;
        }
        if let Some(dirs) = shell.engine().user_dirs() {
            let name = admission.agent.name.clone();
            let remembered = state::update(dirs, |s| {
                if !s.allowed_agents.contains(&name) {
                    s.allowed_agents.push(name.clone());
                }
            });
            if let Err(e) = remembered {
                shell.report(tr!("error-state-file", error = e.to_string()));
            }
        }
        admission.allow();
    }

    /// Whether a file hand-off asked the window to come to the front; asking clears it.
    pub fn take_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.focus)
    }

    /// Between frames: lets in allowed agents and queues the rest for the prompt, runs
    /// every bridged Command waiting, and publishes the open Project.
    pub fn pump(&mut self, shell: &mut Shell) {
        let Some(running) = &self.running else {
            return;
        };
        let admissions: Vec<Admission> = running.admissions.try_iter().collect();
        if !admissions.is_empty() {
            let allowed = self.allowed(shell);
            for admission in admissions {
                match allowed.contains(&admission.agent.name) {
                    true => admission.allow(),
                    false => self.asking.push_back(admission),
                }
            }
        }
        let Some(running) = &self.running else {
            return;
        };
        let calls: Vec<Call> = running.calls.try_iter().collect();
        for call in calls {
            if call.id == protocol::OPEN_FILE {
                let path = call.params["path"].as_str().map(str::to_owned);
                let opened = match path {
                    Some(path) => shell.open_in_place(Path::new(&path)).map(|()| json!({})),
                    None => Err(CommandError::invalid_params(
                        "/path",
                        "path must be a string",
                    )),
                };
                self.focus |= opened.is_ok();
                call.reply(opened);
                continue;
            }
            let source = call.source;
            let _ = call.run(|id, params, signals| shell.run_bridged(source, id, params, signals));
        }
        if let Some(running) = &self.running {
            let _ = running
                .server
                .set_project(reda_control::project_file(shell.engine()));
        }
    }

    /// Closes the port. Agents waiting to be let in are denied; Calls not yet run are
    /// answered `internal`.
    pub fn stop(&mut self) {
        for admission in self.asking.drain(..) {
            admission.deny();
        }
        let Some(Running {
            server,
            calls,
            admissions,
            forwarders,
        }) = self.running.take()
        else {
            return;
        };
        // Dropped first: the server waits for every Call it handed out to be answered.
        drop((calls, admissions));
        server.stop();
        drop(server);
        for forwarder in forwarders {
            let _ = forwarder.join();
        }
    }

    fn open(&mut self, shell: &Shell) {
        self.error = None;
        let Some(dirs) = shell.engine().user_dirs() else {
            return;
        };
        let catalogue = shell.engine().registry().catalogue();
        let project = reda_control::project_file(shell.engine());
        match ControlServer::start_gated(dirs, &catalogue, project) {
            Ok((server, calls, admissions)) => {
                let (calls, call_forwarder) = forward(calls, self.wake.clone());
                let (admissions, admission_forwarder) = forward(admissions, self.wake.clone());
                self.running = Some(Running {
                    server,
                    calls,
                    admissions,
                    forwarders: vec![call_forwarder, admission_forwarder],
                });
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Passes everything from `from` to a new channel, waking the window for each.
fn forward<T: Send + 'static>(
    from: Receiver<T>,
    wake: Arc<Mutex<Option<Wake>>>,
) -> (Receiver<T>, JoinHandle<()>) {
    let (to, receiver) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        for item in from {
            if to.send(item).is_err() {
                return;
            }
            let wake = wake.lock().expect("not poisoned").clone();
            if let Some(wake) = wake {
                wake();
            }
        }
    });
    (receiver, thread)
}

/// Hands `path` (a Project or one of its Document files) to the running instance that has
/// its Project open, which focuses it. `false` means none took it: open it here.
pub fn hand_off(dirs: &UserDirs, path: &Path) -> bool {
    let is_document = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(DocumentKind::from_extension)
        .is_some();
    let project = match is_document {
        true => path.parent().unwrap_or(path),
        false => path,
    };
    let Ok(Some(mut connection)) = client::bridge(dirs, Some(project), Source::Gui) else {
        return false;
    };
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let params: Value = json!({ "path": path });
    connection.run(protocol::OPEN_FILE, params).is_ok()
}
