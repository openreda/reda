//! Trial harness for the `step-io` crate (REDA issue #442). See `../SPEC.md`.

mod board;
mod json;
mod read;
mod transcribe;
mod write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let line = match args.first().map(String::as_str) {
        Some("read") if args.len() == 2 => read::run(&args[1]),
        Some("write") if args.len() >= 3 => {
            let patch_names = !args[3..].iter().any(|a| a == "--no-name-patch");
            write::run(&args[1], &args[2], patch_names)
        }
        // Not in the contract: step-io's own write of a model it read, to see
        // what `step_io::write` does with a non-AP242 input.
        Some("rewrite") if args.len() == 3 => write::rewrite(&args[1], &args[2]),
        _ => {
            eprintln!(
                "usage: step-io-trial read <file.step>\n       \
                 step-io-trial write <out.step> <component.step> [--no-name-patch]\n       \
                 step-io-trial rewrite <out.step> <in.step>"
            );
            std::process::exit(2);
        }
    };
    println!("{line}");
}
