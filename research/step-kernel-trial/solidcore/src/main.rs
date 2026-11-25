//! Trial harness for REDA issue #442: solidcore (the kernel inside SolidCraft) as a STEP
//! reader, mesher and writer. See ../SPEC.md for the contract and README.md for the findings.

mod json;
mod read;
mod write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let line = match args.get(1).map(String::as_str) {
        Some("read") if args.len() == 3 => read::run(&args[2]),
        Some("write") if args.len() == 4 => write::run(&args[2], &args[3]),
        _ => {
            eprintln!("usage: step-trial-solidcore read <file.step> | write <out.step> <component.step>");
            std::process::exit(2);
        }
    };
    println!("{line}");
}
