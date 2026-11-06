//! Drawing the bridge: the status-bar indicator (with the bridge switch, the connected
//! agents and the allowed agents, as on the Preferences dialog's Agents page) and the
//! "Allow this agent?" prompt. The state is in [`Bridge`].

use egui::{Id, Ui};

use crate::bridge::Bridge;
use crate::shell::Shell;
use crate::tr;

/// The status-bar indicator: whether agents can connect and how many are. Its menu turns
/// the bridge on and off and forgets allowed agents. Returns whether "Agent Settings…"
/// was clicked, which opens the Preferences dialog's Agents page.
pub fn indicator(ui: &mut Ui, bridge: &mut Bridge, shell: &mut Shell) -> bool {
    let mut settings = false;
    let agents = bridge.agents();
    let label = match (bridge.is_on(), agents.len()) {
        (false, _) => tr!("bridge-indicator-off"),
        (true, 0) => tr!("bridge-indicator-on"),
        (true, count) => tr!("bridge-indicator-connected", count = count),
    };
    ui.menu_button(label, |ui| {
        let mut on = bridge.is_on();
        if ui.checkbox(&mut on, tr!("bridge-switch")).changed() {
            bridge.set_enabled(shell, on);
        }
        if let Some(error) = bridge.error() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                tr!("bridge-error", error = error),
            );
        }
        if !agents.is_empty() {
            ui.separator();
            ui.weak(tr!("bridge-connected"));
            for agent in &agents {
                ui.label(format!("{} ({})", agent.name, agent.source.label()));
            }
        }
        ui.separator();
        ui.weak(tr!("bridge-allowed"));
        let allowed = bridge.allowed(shell);
        if allowed.is_empty() {
            ui.label(tr!("bridge-none-allowed"));
        }
        for name in allowed {
            ui.horizontal(|ui| {
                ui.label(name.as_str());
                if ui.small_button(tr!("bridge-forget")).clicked() {
                    bridge.forget_agent(shell, &name);
                }
            });
        }
        ui.separator();
        if ui.button(tr!("bridge-settings")).clicked() {
            settings = true;
            ui.close();
        }
    });
    settings
}

/// "Allow this agent?" for the first agent waiting, if any. Bridged Commands of agents
/// already in keep running while it is up.
pub fn ask(ctx: &egui::Context, bridge: &mut Bridge, shell: &mut Shell) {
    let Some(agent) = bridge.asking().cloned() else {
        return;
    };
    let mut answer = None;
    crate::modal::Dialog::new(Id::NULL.with("bridge-allow-agent"), tr!("bridge-ask-title")).show(
        ctx,
        |ui| {
            ui.label(tr!(
                "bridge-ask-body",
                agent = agent.name.as_str(),
                source = agent.source.label()
            ));
            ui.horizontal(|ui| {
                if ui.button(tr!("bridge-allow")).clicked() {
                    answer = Some(true);
                }
                if ui.button(tr!("bridge-deny")).clicked() {
                    answer = Some(false);
                }
            });
        },
    );
    if let Some(allow) = answer {
        bridge.answer(shell, allow);
    }
}
