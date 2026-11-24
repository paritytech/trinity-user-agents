//! M0 T1 check: run a script in the QuickJS guest under wasmi on macOS.
//!
//! Usage: qjs-runner <guest.wasm> <script.js> [--fuel-per-turn N] [--memory-mib N]
//!        [--native-stack-mib N] [--replay corpus.bin] [--instances N]
//!
//! Every outbound frame is echoed back as an inbound frame. Timers run on
//! wall-clock time. The process exits non-zero if the guest traps.

use std::process::ExitCode;

use qjs_runner::Limits;

struct Options {
    guest: String,
    script: String,
    limits: Limits,
    native_stack_mib: usize,
    replay: Option<String>,
    instances: usize,
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        },
    };
    let outcome = std::thread::Builder::new()
        .name("guest".into())
        .stack_size(options.native_stack_mib << 20)
        .spawn(move || run(&options))
        .expect("spawn guest thread")
        .join()
        .expect("guest thread panicked");
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[runner] guest stopped: {error}");
            ExitCode::FAILURE
        },
    }
}

fn run(options: &Options) -> Result<(), String> {
    let wasm = std::fs::read(&options.guest).map_err(|error| format!("{}: {error}", options.guest))?;
    let source = std::fs::read(&options.script).map_err(|error| format!("{}: {error}", options.script))?;
    let result = if options.instances > 0 {
        qjs_runner::instances(&wasm, &source, options.instances, &options.limits)
    } else if let Some(corpus) = &options.replay {
        qjs_runner::replay(&wasm, &source, corpus, &options.limits)
    } else {
        return qjs_runner::run_to_completion(&wasm, &source, &options.limits).map(drop).map_err(|e| e.to_string());
    };
    let line = result.map_err(|error| error.to_string())?;
    println!("RESULT {line}");
    if line.contains("\"mismatches\":") && !line.contains("\"mismatches\":0") {
        return Err("steps did not match the corpus".into());
    }
    Ok(())
}

fn parse_options() -> Result<Options, String> {
    let usage = "usage: qjs-runner <guest.wasm> <script.js> [--fuel-per-turn N] [--memory-mib N] [--native-stack-mib N] [--replay corpus.bin] [--instances N]"
        .to_string();
    let mut args = std::env::args().skip(1);
    let guest = args.next().ok_or_else(|| usage.clone())?;
    let script = args.next().ok_or_else(|| usage.clone())?;
    let mut options =
        Options { guest, script, limits: Limits::default(), native_stack_mib: 8, replay: None, instances: 0 };
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| usage.clone())?;
        if flag == "--replay" {
            options.replay = Some(value);
            continue;
        }
        let number: u64 = value.parse().map_err(|_| usage.clone())?;
        match flag.as_str() {
            "--instances" => options.instances = number as usize,
            "--fuel-per-turn" => options.limits.fuel_per_turn = number,
            "--memory-mib" => options.limits.memory_bytes = (number as usize) << 20,
            "--native-stack-mib" => options.native_stack_mib = number as usize,
            _ => return Err(usage),
        }
    }
    Ok(options)
}
