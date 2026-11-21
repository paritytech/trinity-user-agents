//! E2 probe: the same guest code (prelude, bindings, QuickJS) compiled
//! natively, so its numbers against the wasm guest show what the wasm layer
//! costs. Runs a micro workload to completion: evaluate, drain jobs, fire
//! timers on wall-clock time.
//!
//! Usage: native-probe <script.js>

use std::time::{Duration, Instant};

use qjs_guest::host::native::{take_outbound, take_timers};

fn main() {
    let path = std::env::args().nth(1).expect("usage: native-probe <script.js>");
    let source = std::fs::read(&path).expect("read script");
    let started = Instant::now();
    let status = unsafe { qjs_guest::init(source.as_ptr(), source.len()) };
    drain();
    eprintln!("[native] init to idle {:?}, status {status}", started.elapsed());

    loop {
        let timers = take_timers();
        let Some((&id, &due_ms)) = timers.iter().min_by(|left, right| left.1.total_cmp(right.1)) else { break };
        let now_ms = qjs_guest::host::native::now_ms();
        if due_ms > now_ms {
            std::thread::sleep(Duration::from_secs_f64((due_ms - now_ms) / 1000.0));
        }
        for (other, other_due) in timers.iter().filter(|(other, _)| **other != id) {
            qjs_guest::host::native::timer_set(*other, other_due - qjs_guest::host::native::now_ms());
        }
        qjs_guest::on_timer(id);
        drain();
    }
}

/// Run jobs and echo frames back until nothing is left to do right now.
fn drain() {
    loop {
        qjs_guest::run_jobs();
        let outbound = take_outbound();
        if outbound.is_empty() {
            return;
        }
        for frame in outbound {
            unsafe { qjs_guest::on_frame(frame.as_ptr(), frame.len()) };
        }
    }
}
