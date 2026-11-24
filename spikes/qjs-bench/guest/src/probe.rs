//! E2 measurements: the same guest code (prelude, bindings, QuickJS)
//! compiled natively, so its numbers against the wasm guest show what the
//! wasm layer costs. Each returns the RESULT JSON it measured.

use std::time::{Duration, Instant};

use crate::{
    host::native::{now_ms, take_outbound, take_timers, timer_set},
    Engine,
};

const ENGINE: &str = "quickjs-native";

fn boot(source: Vec<u8>) -> Result<(Engine, Duration), String> {
    let started = Instant::now();
    let (engine, status) = Engine::new(source);
    engine.run_jobs();
    if status != 0 {
        return Err("worker script threw".into());
    }
    Ok((engine, started.elapsed()))
}

/// Boot `count` engines on the worker bundle, keeping each alive, and report
/// cold start and the memory each one adds to the process.
pub fn instances(source: &[u8], count: usize) -> Result<String, String> {
    let before = bench_common::footprint_bytes();
    let mut engines = Vec::with_capacity(count);
    let mut boot_ms = Vec::with_capacity(count);
    let mut footprints = Vec::with_capacity(count);
    for _ in 0..count {
        let (engine, booted_in) = boot(source.to_vec())?;
        boot_ms.push(booted_in.as_secs_f64() * 1000.0);
        footprints.push(bench_common::footprint_bytes());
        engines.push(engine);
    }
    let first_mib = bench_common::mib(footprints[0] - before);
    let rest_mib = bench_common::mib(footprints[count - 1] - footprints[0]) / (count - 1).max(1) as f64;
    let first_ms = boot_ms[0];
    let (warm_p50, _) = bench_common::percentiles(&mut boot_ms[1..].to_vec());
    Ok(format!(
        "{{\"engine\":\"{ENGINE}\",\"firstBootMs\":{first_ms:.1},\"bootP50Ms\":{warm_p50:.1},\"firstInstanceMiB\":{first_mib:.2},\"perInstanceMiB\":{rest_mib:.2},\"instances\":{count}}}"
    ))
}

/// Replay the corpus, checking every outbound frame byte for byte and timing
/// each render request until its tree comes back.
pub fn replay(source: &[u8], corpus: &str) -> Result<String, String> {
    let (engine, booted_in) = boot(source.to_vec())?;
    take_outbound();
    let steps = bench_common::load_corpus(corpus);
    let mut render_ms = Vec::new();
    let mut mismatches = 0;
    for step in &steps {
        let started = Instant::now();
        engine.deliver(step.inbound.clone());
        engine.run_jobs();
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        if take_outbound() != step.outbound {
            mismatches += 1;
        }
        if bench_common::is_render_start(&step.inbound) {
            render_ms.push(elapsed);
        }
    }
    let (p50, p95) = bench_common::percentiles(&mut render_ms);
    Ok(format!(
        "{{\"engine\":\"{ENGINE}\",\"bootMs\":{:.1},\"steps\":{},\"mismatches\":{mismatches},\"renderSamples\":{},\"renderP50Ms\":{p50:.3},\"renderP95Ms\":{p95:.3}}}",
        booted_in.as_secs_f64() * 1000.0,
        steps.len(),
        render_ms.len(),
    ))
}

/// Run a micro workload to completion: evaluate, drain jobs, echo frames,
/// fire timers on wall-clock time. Returns the time from start to idle.
pub fn run_to_completion(source: &[u8]) -> Result<Duration, String> {
    let started = Instant::now();
    let (engine, status) = Engine::new(source.to_vec());
    drain(&engine);
    let init_in = started.elapsed();
    if status != 0 {
        return Err("worker script threw".into());
    }
    loop {
        let timers = take_timers();
        let Some((&id, &due_ms)) = timers.iter().min_by(|left, right| left.1.total_cmp(right.1)) else { break };
        if due_ms > now_ms() {
            std::thread::sleep(Duration::from_secs_f64((due_ms - now_ms()) / 1000.0));
        }
        for (other, other_due) in timers.iter().filter(|(other, _)| **other != id) {
            timer_set(*other, other_due - now_ms());
        }
        engine.fire_timer(id);
        drain(&engine);
    }
    Ok(init_in)
}

/// Run jobs and echo frames back until nothing is left to do right now.
fn drain(engine: &Engine) {
    loop {
        engine.run_jobs();
        let outbound = take_outbound();
        if outbound.is_empty() {
            return;
        }
        for frame in outbound {
            engine.deliver(frame);
        }
    }
}
