//! The wasm2c-compiled QuickJS guest on the Mac.
//!
//! Usage: qjs-runner-wasm2c <script.js> [--memory-mib N] [--native-stack-mib N]
//!        [--replay corpus.bin] [--instances N] [--guard-capacity N]

use std::process::ExitCode;

use qjs_runner_wasm2c::Limits;

struct Options {
    script: String,
    limits: Limits,
    native_stack_mib: usize,
    replay: Option<String>,
    instances: usize,
    guard_capacity: usize,
}

fn parse_options() -> Result<Options, String> {
    let usage = "usage: qjs-runner-wasm2c <script.js> [--memory-mib N] [--native-stack-mib N] [--replay corpus.bin] [--instances N] [--guard-capacity N]".to_string();
    let mut args = std::env::args().skip(1);
    let script = args.next().ok_or_else(|| usage.clone())?;
    let mut options = Options {
        script,
        limits: Limits::default(),
        native_stack_mib: 8,
        replay: None,
        instances: 0,
        guard_capacity: 0,
    };
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| usage.clone())?;
        if flag == "--replay" {
            options.replay = Some(value);
            continue;
        }
        let number: usize = value.parse().map_err(|_| usage.clone())?;
        match flag.as_str() {
            "--instances" => options.instances = number,
            "--guard-capacity" => options.guard_capacity = number,
            "--memory-mib" => options.limits.memory_bytes = number << 20,
            "--native-stack-mib" => options.native_stack_mib = number,
            _ => return Err(usage),
        }
    }
    Ok(options)
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
        .spawn(move || {
            let outcome = run(&options);
            qjs_runner_wasm2c::release_thread();
            outcome
        })
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
    if options.guard_capacity > 0 {
        println!("RESULT {}", qjs_runner_wasm2c::guard_capacity(options.guard_capacity));
        return Ok(());
    }
    let source = std::fs::read(&options.script).map_err(|error| format!("{}: {error}", options.script))?;
    let line = if options.instances > 0 {
        qjs_runner_wasm2c::instances(&source, options.instances, &options.limits)?
    } else if let Some(corpus) = &options.replay {
        qjs_runner_wasm2c::replay(&source, corpus, &options.limits)?
    } else {
        return qjs_runner_wasm2c::run_to_completion(&source, &options.limits).map(drop);
    };
    println!("RESULT {line}");
    if line.contains("\"mismatches\":") && !line.contains("\"mismatches\":0") {
        return Err("steps did not match the corpus".into());
    }
    Ok(())
}
