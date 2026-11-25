//! Trial harness for the kernel-free STEP route with no third-party crate (REDA issue #442):
//! an in-house writer for the board and a text-level re-embedder for component models.

mod board;
mod check;
mod embed;
mod p21;

use board::{AP214_SCHEMA, AP242_SCHEMA, Emitter};

pub fn json_string(s: &str) -> String {
    let mut o = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

struct WriteOptions {
    out: String,
    component: Option<String>,
    ap242: bool,
    keep_app_context: bool,
}

/// Builds the file and returns `(bytes, notes)`.
fn build(o: &WriteOptions) -> Result<(Vec<u8>, Vec<String>), String> {
    let schema = if o.ap242 { AP242_SCHEMA } else { AP214_SCHEMA };
    let out_ap = embed::classify_schema(schema);
    let mut e = Emitter::new();
    let ctx = board::write_contexts(&mut e, o.ap242);
    let pcb = board::write_board(&mut e, &ctx);
    let mut notes = Vec::new();
    let Some(component) = &o.component else {
        e.add(format!(
            "PRODUCT_RELATED_PRODUCT_CATEGORY('part',$,(#{}))",
            pcb.product
        ));
        notes.push("board only: one PCB product, no assembly".to_string());
        return Ok((
            board::finish(&e, schema, "REDA step-kernel-trial: PCB"),
            notes,
        ));
    };

    let bytes =
        std::fs::read(component).map_err(|err| format!("cannot read {component}: {err}"))?;
    let file =
        p21::parse_file(&bytes).map_err(|err| format!("cannot tokenize {component}: {err}"))?;
    let comp_schema = embed::declared_schema(&file);
    let comp_ap = embed::classify_schema(&comp_schema);

    // Placements in the assembly frame: the PCB as drawn, U1 turned 90 degrees, U2 as drawn.
    let asm = embed::write_assembly(
        &mut e,
        &ctx,
        "trial-board",
        &[
            ([0.0; 3], 0.0),
            ([20.0, 10.0, board::THICKNESS], 90.0),
            ([35.0, 20.0, board::THICKNESS], 0.0),
        ],
    );
    e.add(format!(
        "PRODUCT_RELATED_PRODUCT_CATEGORY('part',$,(#{},#{}))",
        asm.product, pcb.product
    ));
    embed::write_occurrence(&mut e, &asm, 0, "PCB", &pcb);
    let embedded = embed::embed_component(&mut e, &ctx, &file, o.keep_app_context)?;
    embed::write_occurrence(&mut e, &asm, 1, "U1", &embedded.product);
    embed::write_occurrence(&mut e, &asm, 2, "U2", &embedded.product);

    notes.push(format!(
        "component declares {comp_ap} ('{comp_schema}'), output declares {out_ap}: {}",
        if comp_ap == out_ap {
            "same protocol; edition strings may differ".to_string()
        } else {
            format!("the component's entities are {comp_ap} text inside a file that declares {out_ap}; nothing was converted")
        }
    ));
    notes.extend(embedded.notes);
    notes.push(
        "U1 and U2 share one product definition; the component's units contexts are kept".into(),
    );
    Ok((
        board::finish(&e, schema, "REDA step-kernel-trial: trial-board"),
        notes,
    ))
}

fn write_command(args: &[String]) -> i32 {
    let mut positional = Vec::new();
    let (mut ap242, mut board_only, mut keep_app_context) = (false, false, false);
    for a in args {
        match a.as_str() {
            "--ap242" => ap242 = true,
            "--board-only" => board_only = true,
            "--keep-app-context" => keep_app_context = true,
            _ => positional.push(a.clone()),
        }
    }
    let usage = "usage: write [--ap242] [--keep-app-context] <out.step> <component.step> | write [--ap242] --board-only <out.step>";
    let schema = if ap242 { AP242_SCHEMA } else { AP214_SCHEMA };
    let result = if positional.len() != if board_only { 1 } else { 2 } {
        Err(usage.to_string())
    } else {
        let o = WriteOptions {
            out: positional[0].clone(),
            component: positional.get(1).cloned(),
            ap242,
            keep_app_context,
        };
        build(&o).and_then(|(bytes, notes)| {
            std::fs::write(&o.out, &bytes)
                .map_err(|err| format!("cannot write {}: {err}", o.out))?;
            Ok((bytes.len(), notes))
        })
    };
    match result {
        Ok((bytes, notes)) => {
            println!(
                "{{\"ok\":true,\"error\":null,\"bytes\":{bytes},\"schema\":{},\"notes\":{}}}",
                json_string(schema),
                json_string(&notes.join("; "))
            );
            0
        }
        Err(err) => {
            let short: String = err.chars().take(300).collect();
            println!(
                "{{\"ok\":false,\"error\":{},\"bytes\":0,\"schema\":{},\"notes\":\"nothing written\"}}",
                json_string(&short),
                json_string(schema)
            );
            0
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("write") => write_command(&args[1..]),
        Some("check") if args.len() == 2 => check::run(&args[1]),
        _ => {
            eprintln!("usage: step-trial-inhouse write ... | check <file.step>");
            2
        }
    };
    std::process::exit(code);
}
