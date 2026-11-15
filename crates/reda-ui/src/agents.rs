//! Drawing the bridge: the status-bar indicator (with the bridge switch, the connected
//! agents and the allowed agents, as on the Preferences dialog's Agents page) and the
//! "Allow this agent?" prompt. The state is in [`Bridge`].

use egui::{Id, Ui};

use crate::bridge::Bridge;
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets;

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
    let t = ui.tokens();
    let face = ui.add(widgets::status_item(label));
    widgets::menu(&face).stay_open().show(|ui| {
        let mut on = bridge.is_on();
        if ui
            .add(widgets::checkbox(&mut on, tr!("bridge-switch")))
            .changed()
        {
            bridge.set_enabled(shell, on);
        }
        if let Some(error) = bridge.error() {
            let error = tr!("bridge-error", error = error);
            ui.label(t.ty.body.rich(error).color(t.color.error.text));
        }
        let heading = |ui: &mut Ui, text: String| {
            widgets::menu_separator(ui);
            ui.label(t.ty.small.rich(text).color(t.color.text.muted));
        };
        if !agents.is_empty() {
            heading(ui, tr!("bridge-connected"));
            for agent in &agents {
                ui.label(format!("{} ({})", agent.name, agent.source.label()));
            }
        }
        heading(ui, tr!("bridge-allowed"));
        let allowed = bridge.allowed(shell);
        if allowed.is_empty() {
            ui.label(tr!("bridge-none-allowed"));
        }
        for name in allowed {
            ui.horizontal(|ui| {
                ui.label(name.as_str());
                if ui
                    .add(widgets::button(tr!("bridge-forget")).flat())
                    .clicked()
                {
                    bridge.forget_agent(shell, &name);
                }
            });
        }
        widgets::menu_separator(ui);
        if ui.add(widgets::menu_item(tr!("bridge-settings"))).clicked() {
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
            crate::modal::buttons(ui, |ui| {
                if ui
                    .add(widgets::button(tr!("bridge-allow")).primary())
                    .clicked()
                {
                    answer = Some(true);
                }
                if ui.add(widgets::button(tr!("bridge-deny"))).clicked() {
                    answer = Some(false);
                }
            });
        },
    );
    if let Some(allow) = answer {
        bridge.answer(shell, allow);
    }
}
