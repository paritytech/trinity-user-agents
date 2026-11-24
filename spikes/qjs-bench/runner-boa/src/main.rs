//! The worker scripts on Boa, on the Mac.
//!
//! Usage: qjs-runner-boa <script.js> [--replay corpus.bin | --instances N]

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: qjs-runner-boa <script.js> [--replay corpus.bin | --instances N]");
        return ExitCode::from(2);
    };
    let (mode, value) = (args.next(), args.next());
    let outcome = std::thread::Builder::new()
        .name("guest".into())
        .stack_size(64 << 20)
        .spawn(move || {
            let source = std::fs::read(&path).map_err(|error| format!("{path}: {error}"))?;
            match (mode.as_deref(), value) {
                (Some("--replay"), Some(corpus)) => qjs_runner_boa::replay(&source, &corpus).map(Some),
                (Some("--instances"), Some(count)) => {
                    qjs_runner_boa::instances(&source, count.parse().map_err(|_| "instance count")?).map(Some)
                },
                _ => qjs_runner_boa::run_to_completion(&source).map(|init_in| {
                    eprintln!("[boa] init to idle {init_in:?}");
                    None
                }),
            }
        })
        .expect("spawn guest thread")
        .join()
        .expect("guest thread panicked");
    match outcome {
        Ok(Some(line)) => {
            println!("RESULT {line}");
            if line.contains("\"mismatches\":") && !line.contains("\"mismatches\":0") {
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        },
        Ok(None) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[boa] stopped: {error}");
            ExitCode::FAILURE
        },
    }
}
