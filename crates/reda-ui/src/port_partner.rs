//! Jump to a Port's partner (docs/spec/v0.1/schematic.md#port-cross-references): the Ports
//! of the same name on other sheets. `Mod`+double-click on a Port or the canvas menu runs
//! the view Command `schematic.port.go_to_partner`; with one partner (or the one picked)
//! the Shell opens that sheet, zooms to the partner and highlights its Net. With several,
//! the window asks which, in a small menu ([`menu`]), whose choice runs the Command again
//! with `partner`.

use egui::Id;
use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::Uuid;
use reda_engine::schematic::cross_ref::{Partner, partners};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::tr;

/// The partners waiting for the user to pick one, in the engine's view state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartnerMenu {
    /// The Port jumped from.
    pub port: Option<Uuid>,
    pub partners: Vec<Partner>,
}

pub fn register_all(registry: &mut Registry) {
    registry.add(GoToPartner);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GoToPartnerParams {
    /// Objects to jump from: the first Port among them.
    objects: Vec<Uuid>,
    /// The partner Port to go to; left out, the only partner (or a menu of them).
    #[serde(default)]
    partner: Option<Uuid>,
}

/// The Port's partners and the one gone to, if any.
#[derive(Serialize, JsonSchema)]
pub struct WentToPartner {
    pub port: Uuid,
    pub partners: Vec<Partner>,
    /// The partner gone to; null while the window asks which.
    pub chosen: Option<Partner>,
}

struct GoToPartner;

impl Command for GoToPartner {
    const META: Meta = Meta {
        id: "schematic.port.go_to_partner",
        title: "Go to Port Partner",
        description: "Goes to a Port's partner, a Port of the same name on another sheet: \
                      opens its sheet, zooms to it and highlights its Net. With several \
                      partners and no `partner` given, the window asks which. View state \
                      only.",
        category: "schematic",
        icon: "net",
        kind: Kind::View,
    };
    type Params = GoToPartnerParams;
    type Output = WentToPartner;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: GoToPartnerParams,
    ) -> Result<WentToPartner, CommandError> {
        let project = cx.project()?;
        let found = p.objects.iter().find_map(|o| {
            let found = partners(project, *o)?;
            Some((*o, found))
        });
        let Some((port, partners)) = found else {
            return Err(CommandError::rejected("none of the objects is a Port"));
        };
        if partners.is_empty() {
            return Err(CommandError::rejected(
                "this Port has no partner: no other sheet has a Port of its name",
            ));
        }
        let chosen = match p.partner {
            Some(partner) => Some(
                (partners.iter().find(|q| q.port == partner).cloned()).ok_or_else(|| {
                    CommandError::invalid_params("/partner", "not a partner of this Port")
                })?,
            ),
            None if partners.len() == 1 => partners.first().cloned(),
            None => None,
        };
        *cx.view::<PartnerMenu>()? = match chosen {
            Some(_) => PartnerMenu::default(),
            None => PartnerMenu {
                port: Some(port),
                partners: partners.clone(),
            },
        };
        Ok(WentToPartner {
            port,
            partners,
            chosen,
        })
    }
}

/// After `schematic.port.go_to_partner`: goes to the chosen partner.
pub(crate) fn after_go(shell: &mut Shell, out: &Value) {
    let Ok(Some(partner)) = serde_json::from_value::<Option<PartnerRef>>(out["chosen"].clone())
    else {
        return;
    };
    if shell
        .run_reporting("document.open", json!({ "document": partner.document }))
        .is_err()
    {
        return;
    }
    let _ = shell.run_reporting(
        "view.zoom_to",
        json!({ "document": partner.document, "objects": [partner.port] }),
    );
    let _ = shell.run_reporting(
        "view.highlight.net_of",
        json!({ "objects": [partner.port] }),
    );
}

/// The fields of a [`Partner`] the jump needs.
#[derive(Deserialize)]
struct PartnerRef {
    port: Uuid,
    document: Uuid,
}

/// The partners waiting for a pick.
pub fn waiting(shell: &Shell) -> Option<PartnerMenu> {
    let menu = shell.engine().view().get::<PartnerMenu>()?;
    (!menu.partners.is_empty()).then(|| menu.clone())
}

/// Drops the partners waiting for a pick.
pub fn dismiss(shell: &mut Shell) {
    if let Some(menu) = shell.engine_mut().view_mut().get_mut::<PartnerMenu>() {
        *menu = PartnerMenu::default();
    }
}

/// A partner's entry in the menu, such as "2 Power: B3".
pub fn entry(partner: &Partner) -> String {
    let place = partner.zone.clone().unwrap_or_else(|| {
        let mm = |v: i64| format!("{:.1}", v as f64 / 1e6);
        format!("{}, {}", mm(partner.at.x.0), mm(partner.at.y.0))
    });
    tr!(
        "port-partner-entry",
        number = partner.sheet_number as i64,
        sheet = partner.sheet_title.clone(),
        place = place
    )
}

/// The menu of partners to go to, while several wait for a pick. Returns the Command
/// a pick runs.
pub fn menu(ctx: &egui::Context, shell: &mut Shell) -> Option<(String, Value)> {
    let menu = waiting(shell)?;
    let port = menu.port?;
    let mut out = None;
    let mut cancel = false;
    let response =
        crate::modal::Dialog::new(Id::NULL.with("port-partners"), tr!("port-partner-title"))
            .width(280.0)
            .show(ctx, |ui| {
                for partner in &menu.partners {
                    if ui.button(entry(partner)).clicked() {
                        out = Some((
                            "schematic.port.go_to_partner".to_owned(),
                            json!({ "objects": [port], "partner": partner.port }),
                        ));
                    }
                }
                ui.separator();
                cancel = ui.button(tr!("dialog-cancel")).clicked();
            });
    if out.is_none() && (cancel || response.should_close()) {
        dismiss(shell);
    }
    out
}

/// `Mod`+double-click on a sheet at `at`: the Port drawn there, if any.
pub fn port_at(shell: &Shell, document: Uuid, at: (f64, f64), tolerance: f64) -> Option<Uuid> {
    let project = shell.engine().project()?;
    let sheet = project.schematic_sheet(document)?;
    crate::selection::picking::objects_at(shell, document, at, tolerance)
        .into_iter()
        .find(|o| sheet.port(*o).is_some())
}
