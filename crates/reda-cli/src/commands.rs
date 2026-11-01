//! `reda commands [--json]`: the Command catalogue, with Presets.
//!
//! `--json` prints the same JSON as the checked-in `docs/commands.json`. Without it, each
//! Command is one line of id and title, followed by a line per Preset (`<id>:<preset>`).

use std::process::ExitCode;

use serde_json::Value;

/// Parses everything after `commands`: whether to print JSON. Errors are usage messages.
pub fn parse(args: &[&str]) -> Result<bool, String> {
    match args {
        [] => Ok(false),
        ["--json"] => Ok(true),
        [other, ..] => Err(format!("unexpected argument '{other}'")),
    }
}

pub fn execute(json: bool) -> ExitCode {
    let catalogue = reda_engine::registry().catalogue();
    if json {
        let text = serde_json::to_string_pretty(&catalogue).expect("JSON values serialise");
        println!("{text}");
        return ExitCode::SUCCESS;
    }
    let mut rows: Vec<(&str, &str)> = Vec::new();
    for command in catalogue["commands"].as_array().into_iter().flatten() {
        rows.push((text(&command["id"]), text(&command["title"])));
        for preset in command["presets"].as_array().into_iter().flatten() {
            rows.push((text(&preset["id"]), text(&preset["title"])));
        }
    }
    let width = rows.iter().map(|(id, _)| id.len()).max().unwrap_or(0);
    for (id, title) in rows {
        println!("{id:<width$}  {title}");
    }
    ExitCode::SUCCESS
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}
