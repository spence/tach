//! Contention benchmark for `SyncedInstant`.
//!
//! All `N` threads hammer `now()` in a loop (with an optional spin between
//! calls) for a fixed window. `SyncedInstant` is the only clock with shared
//! mutable hot-path state (a process-global `fetch_max` on one cache line), so
//! its per-call latency should climb with thread count and its throughput
//! should plateau, while the barrier-free clocks stay flat.
//!
//! Run: `cargo bench --bench contention --features bench-internal`
//!
//! Per-call numbers include the ns-conversion wrapper, so the signal is the
//! *delta* between clocks at a given thread count, not the absolute value.

use std::thread::available_parallelism;
use std::time::Duration;

use tach::bench::{
  ClockSource, ContentionResult, MinstantInstant, QuantaInstant, StdInstant, TachFencedInstant,
  TachInstant, TachSyncedInstant, measure_contention,
};

const WINDOW: Duration = Duration::from_millis(800);

// (label, spin_iters) — 0 = pathological tight loop; 256 ≈ a few hundred ns of
// synthetic work between calls (the amortization axis).
const SPINS: &[(&str, u32)] = &[("tight (0 spin)", 0), ("spaced (256 spin)", 256)];

fn run<C: ClockSource>(threads: usize, spin: u32) -> ContentionResult {
  measure_contention::<C>(threads, WINDOW, spin)
}

fn main() {
  let cores = available_parallelism().map(|n| n.get()).unwrap_or(8);
  // 1,2,4,... up to core count, then one oversubscribed point at 2x.
  let mut threads: Vec<usize> = Vec::new();
  let mut t = 1;
  while t < cores {
    threads.push(t);
    t *= 2;
  }
  threads.push(cores);
  threads.push(cores * 2);
  threads.dedup();

  println!("# contention — {cores} logical cores, {WINDOW:?} window/cell\n");

  for (spin_label, spin) in SPINS {
    println!("## {spin_label}\n");

    // Header
    print!("| threads ");
    for c in ["tach", "tach_synced", "tach_fenced", "std", "quanta", "minstant"] {
      print!("| {c:>11} ");
    }
    println!("|");
    print!("|--------:");
    for _ in 0..6 {
      print!("|------------:");
    }
    println!("|");

    // Per-call latency (ns) rows
    let mut synced_per_call: Vec<(usize, f64)> = Vec::new();
    let mut std_per_call: Vec<(usize, f64)> = Vec::new();
    for &n in &threads {
      let tach = run::<TachInstant>(n, *spin);
      let synced = run::<TachSyncedInstant>(n, *spin);
      let fenced = run::<TachFencedInstant>(n, *spin);
      let std_ = run::<StdInstant>(n, *spin);
      let quanta = run::<QuantaInstant>(n, *spin);
      let minstant = run::<MinstantInstant>(n, *spin);
      synced_per_call.push((n, synced.per_call_ns_mean));
      std_per_call.push((n, std_.per_call_ns_mean));
      println!(
        "| {n:>7} | {:>10.1} | {:>10.1} | {:>10.1} | {:>10.1} | {:>10.1} | {:>10.1} |",
        tach.per_call_ns_mean,
        synced.per_call_ns_mean,
        fenced.per_call_ns_mean,
        std_.per_call_ns_mean,
        quanta.per_call_ns_mean,
        minstant.per_call_ns_mean,
      );
    }
    println!("\n_per-call latency in ns (mean per thread); lower is better_\n");

    // Crossover: first thread count where SyncedInstant per-call > std per-call.
    let crossover = synced_per_call
      .iter()
      .zip(std_per_call.iter())
      .find(|((_, s), (_, st))| s > st)
      .map(|((n, _), _)| *n);
    match crossover {
      Some(n) => println!("→ SyncedInstant crosses std at **{n} threads** (slower beyond)\n"),
      None => println!("→ SyncedInstant stays faster than std across all tested thread counts\n"),
    }
  }

  // Throughput + scaling efficiency for the tight case (the stress case).
  println!("## scaling efficiency (tight, 0 spin)\n");
  println!("Throughput(N) / (N × Throughput(1)). 1.00 = perfect scaling; →0 = serialized.\n");
  print!("| threads ");
  for c in ["tach", "tach_synced", "tach_fenced", "std"] {
    print!("| {c:>11} ");
  }
  println!("|");
  print!("|--------:");
  for _ in 0..4 {
    print!("|------------:");
  }
  println!("|");
  let base_tach = run::<TachInstant>(1, 0).throughput_per_sec;
  let base_synced = run::<TachSyncedInstant>(1, 0).throughput_per_sec;
  let base_fenced = run::<TachFencedInstant>(1, 0).throughput_per_sec;
  let base_std = run::<StdInstant>(1, 0).throughput_per_sec;
  for &n in &threads {
    let eff = |base: f64, tput: f64| tput / (n as f64 * base);
    let tach = run::<TachInstant>(n, 0).throughput_per_sec;
    let synced = run::<TachSyncedInstant>(n, 0).throughput_per_sec;
    let fenced = run::<TachFencedInstant>(n, 0).throughput_per_sec;
    let std_ = run::<StdInstant>(n, 0).throughput_per_sec;
    println!(
      "| {n:>7} | {:>11.2} | {:>11.2} | {:>11.2} | {:>11.2} |",
      eff(base_tach, tach),
      eff(base_synced, synced),
      eff(base_fenced, fenced),
      eff(base_std, std_),
    );
  }
  println!();
}
