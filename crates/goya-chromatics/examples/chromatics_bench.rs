//! Internal-processing timings for §23: reallocation at 100 and 1,000
//! candidates and press-to-intent dispatch. Run with
//! `cargo run --release -p goya-chromatics --example chromatics_bench`.
//!
//! This measures the core alone on the build host. It says nothing about
//! platform event delivery, rendering, or a TV's CPU (§23 "Measurements
//! shall distinguish"); a target runtime must be measured on the device.

use std::time::Instant;

use goya_chromatics::{
    Candidate, Capability, Engine, Policy, Registry, Request, Strategy, Trigger,
};

fn percentiles(mut ns: Vec<u128>) -> (f64, f64, f64) {
    ns.sort_unstable();
    let at = |q: f64| ns[((ns.len() as f64 - 1.0) * q).round() as usize] as f64 / 1000.0;
    (at(0.5), at(0.95), at(0.99))
}

fn candidates(n: usize, salt: u64) -> Vec<Candidate> {
    (0..n)
        .map(|i| {
            let mut c = Candidate::new(&format!("item.{i:05}"), "open", "Item");
            let h = (i as u64 ^ salt).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            c.entity = Some(format!("e{i}"));
            c.rank = Some((h % 997) as u32);
            c.favourite = h.is_multiple_of(13).then_some((h % 50) as u32);
            c.probability = (h % 1000) as u32;
            c.cost = 1 + (h % 12) as u32;
            c.urgency = (h % 300) as i64;
            c
        })
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let iterations = 2000;
    println!(
        "goya-chromatics {} internal processing, {iterations} iterations",
        goya_chromatics::CONTRACT
    );
    println!(
        "{:<52} {:>9} {:>9} {:>9}",
        "case", "p50 µs", "p95 µs", "p99 µs"
    );
    for n in [20, 100, 1000] {
        for (name, policy, trigger) in [
            (
                "utility, background (inertia)",
                Policy::new(Strategy::Utility),
                Trigger::Background,
            ),
            (
                "preferential→ranked, policy change",
                Policy::new(Strategy::Preferential).with_fallback(Strategy::Ranked),
                Trigger::Policy,
            ),
        ] {
            let mut engine = Engine::new(Capability::webos_colour_keys(), ["open"])?;
            let sets: Vec<Vec<Candidate>> = (0..8).map(|s| candidates(n, s)).collect();
            let mut times = Vec::with_capacity(iterations);
            for i in 0..iterations {
                let req = Request {
                    context: "bench",
                    policy: &policy,
                    candidates: &sets[i % sets.len()],
                    trigger,
                    now: i as u64,
                    interacting: false,
                };
                let t = Instant::now();
                let a = engine.allocate(&req)?;
                times.push(t.elapsed().as_nanos());
                std::hint::black_box(a);
            }
            let (p50, p95, p99) = percentiles(times);
            println!(
                "{:<52} {p50:>9.1} {p95:>9.1} {p99:>9.1}",
                format!("{n} candidates, {name}")
            );
        }
    }

    let mut engine = Engine::new(Capability::webos_colour_keys(), ["open"])?;
    let cs = candidates(100, 1);
    let policy = Policy::new(Strategy::Ranked);
    let req = Request {
        context: "bench",
        policy: &policy,
        candidates: &cs,
        trigger: Trigger::Context,
        now: 0,
        interacting: false,
    };
    let mut reg = Registry::new();
    reg.activate("bench");
    reg.publish(engine.allocate(&req)?.generation);
    let allow = |_: &str, _: Option<&str>| true;
    let mut times = Vec::with_capacity(iterations * 10);
    for i in 0..iterations * 10 {
        let slot = (i % 4) as u8;
        let t = Instant::now();
        reg.press(slot);
        let d = reg.release(slot, &allow);
        times.push(t.elapsed().as_nanos());
        std::hint::black_box(d);
    }
    let (p50, p95, p99) = percentiles(times);
    println!(
        "{:<52} {p50:>9.2} {p95:>9.2} {p99:>9.2}",
        "press→release dispatch"
    );
    Ok(())
}
