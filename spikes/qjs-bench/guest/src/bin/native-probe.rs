//! E2: the same guest code (prelude, bindings, QuickJS) compiled natively, so
//! its numbers against the wasm guest show what the wasm layer costs.
//!
//! Usage: native-probe <script.js> [--replay corpus.bin | --instances N]
//!
//! Without a mode it runs a micro workload to completion: evaluate, drain
//! jobs, echo frames, fire timers on wall-clock time.

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use qjs_guest::probe;

    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: native-probe <script.js> [--replay corpus.bin | --instances N]");
    let source = std::fs::read(&path).expect("read script");
    let outcome = match (args.next().as_deref(), args.next()) {
        (Some("--replay"), Some(corpus)) => probe::replay(&source, &corpus),
        (Some("--instances"), Some(count)) => probe::instances(&source, count.parse().expect("instance count")),
        _ => probe::run_to_completion(&source).map(|init_in| format!("init to idle {init_in:?}")),
    };
    match outcome {
        Ok(line) if line.starts_with('{') => println!("RESULT {line}"),
        Ok(line) => eprintln!("[native] {line}"),
        Err(error) => panic!("{error}"),
    }
}
