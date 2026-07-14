//! Monotonicity + drift bench across 7 clock sources. Output is a single
//! JSON document per the `tach-skew-bench/v1` schema, intended for
//! `benches/report.py --skewmono-json` consumption.
//!
//! Run modes:
//!   --mode fast   per-thread + cross-thread + skew-1s  (~50s)
//!   --mode drift  skew-1m only                          (~5m per clock)
//!   --mode all    everything                            (default; ~35m+)
//!   --mode sync-order synchronization-order only
//!
//! Useful args:
//!   --cell <name>           cell identifier for the JSON header
//!   --only-clock <name>     restrict to a single clock (used for the
//!                           recalibrate-background variant that requires
//!                           its own build)
//!   --threads <N>           cross-thread test fanout (default num_cpus)
//!   --duration <secs>       per-thread + cross-thread duration (default 10)
//!   --skew-1m-samples <N>   skew-1m sample count (default 5)
//!   --skew-1s-samples <N>   skew-1s sample count (default 30)
//!   --output <path>         write JSON here (default stdout)

use std::collections::BTreeMap;
use std::env;
use std::process::Command;
use std::time::Duration;

use tach::bench::{
  CellReport, ClockReport, ClockSource, FastantInstant, HostInfo, MinstantInstant,
  OrderedVerifyPlacement, OrderedVerifyReport, QuantaInstant, SkewResult, StdInstant,
  SyncOrderResult, TachInstant, TachOrderedInstant, available_core_ids, measure_cross_thread,
  measure_per_thread, measure_skew, measure_synchronization_order,
  measure_synchronization_order_pinned, tach_freq_hz, tach_used_cpuid_15h, unix_ns_now,
};

#[cfg(feature = "recalibrate-background")]
use tach::bench::TachInstantRecal;

const ALL_CLOCKS: &[&str] = &[
  "tach",
  "tach_ordered",
  #[cfg(feature = "recalibrate-background")]
  "tach_recal",
  "std",
  "quanta",
  "minstant",
  "fastant",
];

struct Args {
  mode: Mode,
  cell: String,
  only_clock: Option<String>,
  threads: usize,
  duration: Duration,
  skew_1s_samples: usize,
  skew_1m_samples: usize,
  output: Option<String>,
  /// ordered-verify only: explicit adversarial core ids, e.g. "0,48".
  pin: Option<String>,
}

enum Mode {
  Fast,
  Drift,
  All,
  SyncOrder,
  OrderedVerify,
}

fn main() {
  let args = parse_args();

  // ordered-verify builds its own report shape (placements × clocks), not the
  // per-clock CellReport, so it short-circuits the standard path.
  if matches!(args.mode, Mode::OrderedVerify) {
    run_ordered_verify(&args);
    return;
  }

  // Warmup tach freq + cpuid info before anything else so the report header
  // is filled correctly.
  let freq = tach_freq_hz();
  let used_cpuid = tach_used_cpuid_15h();

  let mut clocks: BTreeMap<String, ClockReport> = BTreeMap::new();
  for &name in ALL_CLOCKS {
    if let Some(only) = &args.only_clock
      && only != name
    {
      continue;
    }
    eprintln!("=== {} ===", name);
    let report = run_clock(name, &args);
    clocks.insert(name.to_string(), report);
  }

  let cell_report = CellReport {
    schema: "tach-skew-bench/v1",
    cell: args.cell,
    target_triple: target_triple(),
    started_at_unix_ns: unix_ns_now(),
    host: gather_host_info(),
    tach_freq_hz: freq,
    tach_used_cpuid_15h: used_cpuid,
    clocks,
  };

  let json = serde_json::to_string_pretty(&cell_report).expect("serialize CellReport");
  match args.output {
    Some(path) => std::fs::write(&path, json).expect("write output"),
    None => println!("{json}"),
  }
}

fn run_clock(name: &str, args: &Args) -> ClockReport {
  match name {
    "tach" => run_for::<TachInstant>(args),
    "tach_ordered" => run_for::<TachOrderedInstant>(args),
    #[cfg(feature = "recalibrate-background")]
    "tach_recal" => run_for::<TachInstantRecal>(args),
    "std" => run_for::<StdInstant>(args),
    "quanta" => run_for::<QuantaInstant>(args),
    "minstant" => run_for::<MinstantInstant>(args),
    "fastant" => run_for::<FastantInstant>(args),
    other => panic!("unknown clock {other:?}"),
  }
}

fn run_for<C: ClockSource>(args: &Args) -> ClockReport {
  let backed_by_arch_counter = C::backed_by_arch_counter();

  let (per_thread, cross_thread, synchronization_order, skew_1s) = match args.mode {
    Mode::Drift => (empty_per_thread::<C>(), empty_cross_thread::<C>(), None, empty_skew_1s::<C>()),
    Mode::SyncOrder => {
      eprintln!("  synchronization-order ({} threads, {:?})...", args.threads, args.duration);
      let st = measure_synchronization_order::<C>(args.threads, args.duration);
      eprintln!(
        "    {} contract violations (max {} ns) / {} reads",
        st.total_violations, st.max_violation_ns, st.total_reads
      );
      (empty_per_thread::<C>(), empty_cross_thread::<C>(), Some(st), empty_skew_1s::<C>())
    }
    Mode::Fast | Mode::All => {
      eprintln!("  per-thread ({:?})...", args.duration);
      let pt = measure_per_thread::<C>(args.duration);
      eprintln!("    {} violations / {} reads", pt.violations, pt.total_reads);

      eprintln!("  cross-thread ({} threads, {:?})...", args.threads, args.duration);
      let ct = measure_cross_thread::<C>(args.threads, args.duration);
      eprintln!(
        "    {} total violations (max {} ns) / {} reads",
        ct.total_violations, ct.max_violation_ns, ct.total_reads
      );

      // Synchronization-order (load-then-now-then-check) — empirically validates
      // whether the bare clock honors the happens-before-respecting strict
      // monotonicity contract. Runs at the cross-thread duration (no separate
      // budget); the metric only matters at the violations≷0 boundary.
      eprintln!("  synchronization-order ({} threads, {:?})...", args.threads, args.duration);
      let st = measure_synchronization_order::<C>(args.threads, args.duration);
      eprintln!(
        "    {} contract violations (max {} ns) / {} reads",
        st.total_violations, st.max_violation_ns, st.total_reads
      );

      eprintln!("  skew-1s ({} samples)...", args.skew_1s_samples);
      let s1 = measure_skew::<C>(Duration::from_secs(1), args.skew_1s_samples, "1s");
      eprintln!("    median skew: {} ns ({:.2} ppm)", s1.median_skew_ns, s1.median_skew_ppm);
      (pt, ct, Some(st), s1)
    }
    // ordered-verify short-circuits in main() before reaching run_for.
    Mode::OrderedVerify => unreachable!("ordered-verify handled in main"),
  };

  let skew_1m = match args.mode {
    Mode::Fast | Mode::SyncOrder | Mode::OrderedVerify => None,
    Mode::Drift | Mode::All => {
      eprintln!("  skew-1m ({} samples)...", args.skew_1m_samples);
      let s60 = measure_skew::<C>(Duration::from_secs(60), args.skew_1m_samples, "1m");
      eprintln!("    median skew: {} ns ({:.2} ppm)", s60.median_skew_ns, s60.median_skew_ppm);
      Some(s60)
    }
  };

  ClockReport {
    backed_by_arch_counter,
    per_thread,
    cross_thread,
    synchronization_order,
    skew_1s,
    skew_1m,
  }
}

fn empty_per_thread<C: ClockSource>() -> tach::bench::PerThreadResult {
  tach::bench::PerThreadResult {
    clock: C::NAME,
    violations: 0,
    total_reads: 0,
    max_violation_ns: 0,
    duration_ns: 0,
  }
}

fn empty_cross_thread<C: ClockSource>() -> tach::bench::CrossThreadResult {
  tach::bench::CrossThreadResult {
    clock: C::NAME,
    threads: 0,
    violations_per_thread: Vec::new(),
    total_violations: 0,
    total_reads: 0,
    max_violation_ns: 0,
    preemption_dropped: 0,
    duration_ns: 0,
    violation_histogram_ns: Vec::new(),
  }
}

fn empty_skew_1s<C: ClockSource>() -> SkewResult {
  SkewResult {
    clock: C::NAME,
    interval: "1s",
    samples: Vec::new(),
    median_skew_ns: 0,
    min_skew_ns: 0,
    max_skew_ns: 0,
    median_skew_ppm: 0.0,
  }
}

fn parse_args() -> Args {
  let raw: Vec<String> = env::args().collect();
  let mut mode = Mode::All;
  let mut cell = "unknown".to_string();
  let mut only_clock = None;
  let mut threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
  let mut duration = Duration::from_secs(10);
  let mut skew_1s_samples = 30usize;
  let mut skew_1m_samples = 5usize;
  let mut output = None;
  let mut pin = None;

  let mut i = 1;
  while i < raw.len() {
    let arg = raw[i].as_str();
    let next = || raw.get(i + 1).cloned().unwrap_or_default();
    match arg {
      "--mode" => {
        mode = match next().as_str() {
          "fast" => Mode::Fast,
          "drift" => Mode::Drift,
          "all" => Mode::All,
          "sync-order" => Mode::SyncOrder,
          "ordered-verify" => Mode::OrderedVerify,
          other => panic!("unknown mode {other:?}"),
        };
        i += 2;
      }
      "--cell" => {
        cell = next();
        i += 2;
      }
      "--only-clock" => {
        only_clock = Some(next());
        i += 2;
      }
      "--threads" => {
        threads = next().parse().expect("--threads requires an integer");
        i += 2;
      }
      "--duration" => {
        duration = Duration::from_secs(next().parse().expect("--duration requires an integer"));
        i += 2;
      }
      "--skew-1s-samples" => {
        skew_1s_samples = next().parse().expect("--skew-1s-samples requires an integer");
        i += 2;
      }
      "--skew-1m-samples" => {
        skew_1m_samples = next().parse().expect("--skew-1m-samples requires an integer");
        i += 2;
      }
      "--output" => {
        output = Some(next());
        i += 2;
      }
      "--pin" => {
        pin = Some(next());
        i += 2;
      }
      // Criterion-style arg pass-through that we don't honor; allow without panicking.
      "--bench" | "--test" | "--quiet" | "--verbose" | "--nocapture" => {
        i += 1;
      }
      _ if arg.starts_with("--") => {
        // Skip unknown long opts conservatively (e.g. --warm-up-time from criterion users).
        i += 2;
      }
      _ => {
        i += 1;
      }
    }
  }

  Args { mode, cell, only_clock, threads, duration, skew_1s_samples, skew_1m_samples, output, pin }
}

/// `--mode ordered-verify`: run the synchronization-order test under deliberately
/// pinned placements to prove `OrderedInstant` holds across threads (and that the
/// fast comparison crates do not). Bare `tach` is the built-in positive control — it must show
/// violations under a placement, or that placement was inert (the result is then
/// inconclusive, not a pass).
///
/// `--pin a,b,...` supplies an adversarial cross-domain core set (e.g. one core
/// per socket from `lscpu`). A full-span (one thread per reported core) and an
/// oversubscribed-2x placement are always added when the platform reports core
/// ids. Emits a `tach-ordered-verify/v1` report. Honors `--cell`, `--duration`,
/// `--output`.
fn run_ordered_verify(args: &Args) {
  let cores = available_core_ids();
  if cores.is_empty() {
    eprintln!("warning: platform reports no core ids; pinning is a no-op here (e.g. macOS)");
  }

  let mut placements: Vec<(String, Vec<usize>)> = Vec::new();
  if let Some(csv) = &args.pin {
    let pin: Vec<usize> = csv.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    if pin.len() >= 2 {
      placements.push(("adversarial-pair".to_string(), pin));
    } else {
      eprintln!("--pin needs >=2 comma-separated core ids; ignoring {csv:?}");
    }
  }
  if !cores.is_empty() {
    placements.push(("full-span".to_string(), cores.clone()));
    let mut oversub = cores.clone();
    oversub.extend_from_slice(&cores);
    placements.push(("oversubscribed-2x".to_string(), oversub));
  }
  if placements.is_empty() {
    eprintln!("no usable placement (no --pin and no reported cores); aborting");
    std::process::exit(1);
  }

  let mut out: Vec<OrderedVerifyPlacement> = Vec::new();
  for (label, pin) in &placements {
    eprintln!("placement {label}: {} threads on cores {pin:?}", pin.len());
    let mut results: BTreeMap<String, SyncOrderResult> = BTreeMap::new();
    macro_rules! run {
      ($name:literal, $ty:ty) => {{
        eprintln!("  {} ...", $name);
        let r = measure_synchronization_order_pinned::<$ty>(pin, args.duration);
        eprintln!(
          "    {} violations (max {} ns) / {} reads",
          r.total_violations, r.max_violation_ns, r.total_reads
        );
        results.insert($name.to_string(), r);
      }};
    }
    run!("tach", TachInstant);
    run!("tach_ordered", TachOrderedInstant);
    run!("std", StdInstant);
    run!("quanta", QuantaInstant);
    run!("minstant", MinstantInstant);
    run!("fastant", FastantInstant);
    out.push(OrderedVerifyPlacement {
      placement: label.clone(),
      pinned_cores: pin.clone(),
      threads: u32::try_from(pin.len()).unwrap_or(u32::MAX),
      results,
    });
  }

  let report = OrderedVerifyReport {
    schema: "tach-ordered-verify/v1",
    cell: args.cell.clone(),
    target_triple: target_triple(),
    started_at_unix_ns: unix_ns_now(),
    host: gather_host_info(),
    tach_freq_hz: tach_freq_hz(),
    duration_secs_per_run: args.duration.as_secs(),
    placements: out,
  };
  let txt = serde_json::to_string_pretty(&report).expect("serialize ordered-verify report");
  match &args.output {
    Some(path) => std::fs::write(path, txt).expect("write output"),
    None => println!("{txt}"),
  }
}

fn target_triple() -> &'static str {
  // Hard-coded triples per (arch, os). Good enough for the cells we run on.
  match (std::env::consts::ARCH, std::env::consts::OS) {
    ("aarch64", "macos") => "aarch64-apple-darwin",
    ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
    ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
    ("x86_64", "macos") => "x86_64-apple-darwin",
    ("x86_64", "windows") => "x86_64-pc-windows-msvc",
    ("aarch64", "windows") => "aarch64-pc-windows-msvc",
    _ => "unknown-triple",
  }
}

fn gather_host_info() -> HostInfo {
  let num_cpus = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(0);
  let cpu_model = read_cpu_model();
  let kernel = read_kernel();
  HostInfo { cpu_model, num_cpus, kernel }
}

fn read_cpu_model() -> String {
  if cfg!(target_os = "macos") {
    Command::new("sysctl")
      .args(["-n", "machdep.cpu.brand_string"])
      .output()
      .ok()
      .and_then(|o| String::from_utf8(o.stdout).ok())
      .map(|s| s.trim().to_string())
      .unwrap_or_default()
  } else if cfg!(target_os = "linux") {
    std::fs::read_to_string("/proc/cpuinfo")
      .ok()
      .and_then(|s| {
        s.lines()
          .find(|l| l.starts_with("model name") || l.starts_with("Model"))
          .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
      })
      .unwrap_or_default()
  } else if cfg!(target_os = "windows") {
    std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_default()
  } else {
    String::new()
  }
}

fn read_kernel() -> String {
  Command::new("uname")
    .arg("-r")
    .output()
    .ok()
    .and_then(|o| String::from_utf8(o.stdout).ok())
    .map(|s| s.trim().to_string())
    .unwrap_or_default()
}
