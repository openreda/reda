//! The schematic PDF: the Project's sheets drawn from their Scenes, with an outline of
//! sheets and Components and links from each Port Cross-Reference entry to its page.
//! `schematic.export_pdf` and an Output Job's run both write it.

use reda_model::text::display;
use reda_model::{DocumentKind, PdfColour, Project, Uuid};
use reda_render::{Bookmark, Palette, PdfInfo, Theme, schematic_scene_styled, to_pdf};

use crate::preferences::Preferences;
use crate::schematic_style::{Drawn, schematic_style};

/// Every Schematic Sheet of `project`, in Document order.
pub fn sheets(project: &Project) -> Vec<Uuid> {
    project
        .documents()
        .filter(|d| d.kind == DocumentKind::Schematic)
        .map(|d| d.uuid)
        .collect()
}

/// The PDF of `sheets` (in the order given; unknown ones are left out), drawn as
/// `preferences` draw an output, in `colour`, and how many pages it has.
///
/// The outline is the Project title, then a bookmark per sheet ("2 Power", numbered as in
/// the Projects panel), each holding a bookmark per Unit on it in natural sort ("R12 10k",
/// with the Unit Suffix where the Component has several Units), which opens at the Unit.
pub fn schematic_pdf(
    project: &Project,
    preferences: &Preferences,
    sheets: &[Uuid],
    colour: PdfColour,
) -> (Vec<u8>, usize) {
    let style = schematic_style(project, preferences, Drawn::Output);
    let numbers = self::sheets(project);
    let mut scenes = Vec::new();
    let mut marks = Vec::new();
    for &uuid in sheets {
        let Some(sheet) = project.schematic_sheet(uuid) else {
            continue;
        };
        let junctions = reda_schematic::connectivity::derive(sheet).junctions;
        let Some(scene) = schematic_scene_styled(project, uuid, &junctions, &style) else {
            continue;
        };
        let page = scenes.len();
        scenes.push(scene);
        let mut units: Vec<(String, reda_model::geom::Point)> = sheet
            .units()
            .filter_map(|unit| {
                let entry = sheet.part(unit.part)?;
                let suffix = style.unit_suffix.of(&entry.symbol, unit.unit);
                let value = display(unit.fields.get("value"));
                let name = format!("{}{suffix}", unit.designator);
                let title = match value.is_empty() {
                    true => name,
                    false => format!("{name} {value}"),
                };
                Some((title, unit.at))
            })
            .collect();
        units.sort_by(|a, b| reda_schematic::natural_cmp(&a.0, &b.0));
        let number = numbers.iter().position(|s| *s == uuid).map_or(0, |i| i + 1);
        let title = project
            .document(uuid)
            .map(|d| d.title.clone())
            .unwrap_or_default();
        marks.push(Bookmark {
            title: format!("{number} {title}").trim_end().to_owned(),
            page,
            at: None,
            children: units
                .into_iter()
                .map(|(title, at)| Bookmark {
                    title,
                    page,
                    at: Some(at),
                    children: Vec::new(),
                })
                .collect(),
        });
    }
    let props = project.props();
    let title = if props.properties.title.is_empty() {
        props.name.clone()
    } else {
        display(&props.properties.title).into_owned()
    };
    let palette = match colour {
        PdfColour::Colour => Palette::shipped(Theme::Light),
        PdfColour::Mono => Palette::mono(),
    };
    let outline = match scenes.is_empty() {
        true => Vec::new(),
        false => vec![Bookmark {
            title: title.clone(),
            page: 0,
            at: None,
            children: marks,
        }],
    };
    let pages = scenes.len();
    (
        to_pdf(&scenes, &palette, &PdfInfo { title, outline }),
        pages,
    )
}
