//! Records how tach's `Instant` (the adopted bare `CNTVCT_EL0` counter) behaves
//! across a whole-system suspend on Apple Silicon — the open evidence item for
//! ADR-0005's platform-defined suspend semantic (`OBJ-SIMPLIFY-TIMERS` §5.1 d).
//!
//! It samples the bare counter alongside the two XNU reference timelines
//! (`mach_absolute_time` excludes system sleep, `mach_continuous_time` includes
//! it), `std::time::Instant` (macOS `CLOCK_UPTIME_RAW`, excludes sleep), and
//! wall `SystemTime` (includes sleep — the ground truth). Across a real suspend
//! it asserts the bare counter never steps backward and RECORDS which reference
//! the bare counter tracks, i.e. whether tach's `Instant::elapsed()` counts the
//! sleep interval. It records; it does not judge — ADR-0005 leaves the semantic
//! platform-defined and documented, so this run produces that documentation.
//!
//! catalyst ONLY — never sleep the headless mini.
//!
//! Owner-coordinated real run (sleep the machine when prompted, keep it asleep
//! ≥60 s, then wake it):
//!   cargo bench --bench apple_suspend_probe --features bench-internal -- --sleep-secs 90 --repeat 5
//!
//! Dry run (validates calibration and clock reads without suspending; all clocks
//! should agree and no divergence appears):
//!   cargo bench --bench apple_suspend_probe --features bench-internal -- --sleep-secs 3 --repeat 1 --dry-run

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn main() {
  use std::time::{Duration, Instant, SystemTime};

  let bare = tach::bench::apple_aarch64_exact_bare_cntvct;
  let mach_abs = tach::bench::apple_aarch64_exact_mach_absolute;
  let mach_cont = tach::bench::apple_aarch64_exact_mach_continuous;

  let mut sleep_secs = 90_u64;
  let mut repeat = 5_u32;
  let mut dry_run = false;
  let mut args = std::env::args().skip(1);
  while let Some(arg) = args.next() {
    match arg.as_str() {
      "--sleep-secs" => sleep_secs = args.next().and_then(|v| v.parse().ok()).unwrap_or(sleep_secs),
      "--repeat" => repeat = args.next().and_then(|v| v.parse().ok()).unwrap_or(repeat),
      "--dry-run" => dry_run = true,
      // `cargo bench` appends the libtest `--bench` flag to the runner; ignore it.
      "--bench" => {}
      other => {
        eprintln!("unknown arg: {other}");
        std::process::exit(2);
      }
    }
  }

  // Calibrate each counter's ticks/sec against std::Instant over a quiet 1 s
  // window while awake. All three track the same physical counter while awake,
  // so this rate is exact; only across a suspend do their semantics diverge.
  let (b0, a0, c0) = (bare(), mach_abs(), mach_cont());
  let cal_start = Instant::now();
  std::thread::sleep(Duration::from_secs(1));
  let cal = cal_start.elapsed().as_secs_f64();
  let bare_hz = bare().saturating_sub(b0) as f64 / cal;
  let mach_abs_hz = mach_abs().saturating_sub(a0) as f64 / cal;
  let mach_cont_hz = mach_cont().saturating_sub(c0) as f64 / cal;
  println!(
    "calibrated ticks/sec: bare={bare_hz:.0} mach_abs={mach_abs_hz:.0} mach_cont={mach_cont_hz:.0}"
  );

  for i in 1..=repeat {
    let (bare_b, abs_b, cont_b, up_b, wall_b) =
      (bare(), mach_abs(), mach_cont(), Instant::now(), SystemTime::now());
    if dry_run {
      println!("[{i}/{repeat}] dry-run: waiting {sleep_secs}s (no suspend)");
    } else {
      println!(
        "[{i}/{repeat}] >>> SLEEP THE MACHINE NOW: `sudo pmset sleepnow` (asleep ≥60s, then wake). Resampling after {sleep_secs}s."
      );
    }
    std::thread::sleep(Duration::from_secs(sleep_secs));
    let (bare_a, abs_a, cont_a, up_a, wall_a) =
      (bare(), mach_abs(), mach_cont(), Instant::now(), SystemTime::now());

    // The `Instant` monotonic contract: the bare counter must never step back.
    assert!(bare_a >= bare_b, "bare CNTVCT stepped backward: {bare_b} -> {bare_a}");
    assert!(abs_a >= abs_b && cont_a >= cont_b, "a mach timeline stepped backward");

    let bare_ns = bare_a.saturating_sub(bare_b) as f64 / bare_hz * 1e9;
    let abs_ns = abs_a.saturating_sub(abs_b) as f64 / mach_abs_hz * 1e9;
    let cont_ns = cont_a.saturating_sub(cont_b) as f64 / mach_cont_hz * 1e9;
    let uptime_ns = up_a.duration_since(up_b).as_nanos() as f64;
    let wall_ns = wall_a.duration_since(wall_b).map_or(f64::NAN, |d| d.as_nanos() as f64);

    let ms = |ns: f64| ns / 1e6;
    println!(
      "[{i}/{repeat}] elapsed ms: bare={:.1} mach_abs={:.1} mach_cont={:.1} std_uptime={:.1} wall={:.1}",
      ms(bare_ns),
      ms(abs_ns),
      ms(cont_ns),
      ms(uptime_ns),
      ms(wall_ns)
    );

    // Which reference does the bare counter track across the gap: wall time
    // (includes sleep, like mach_continuous) or awake time (excludes sleep,
    // like mach_absolute / CLOCK_UPTIME_RAW)?
    let verdict = if wall_ns.is_nan() {
      "wall unavailable"
    } else if (bare_ns - wall_ns).abs() < (bare_ns - uptime_ns).abs() {
      "bare INCLUDES sleep (tracks continuous/wall)"
    } else {
      "bare EXCLUDES sleep (tracks absolute/uptime)"
    };
    println!("[{i}/{repeat}] verdict: {verdict}");
  }
}

#[cfg(not(all(target_arch = "aarch64", target_os = "macos")))]
fn main() {
  eprintln!("apple_suspend_probe runs only on aarch64-apple-darwin");
}
