//! Apple Silicon wall-clock fixed provider picks.
//!
//! XNU publishes the process's timebase mode in a read-only commpage byte set
//! at `exec` and inherited identically across `fork`. The mode says which EL0
//! counter register the kernel permits; reading a register the mode forbids
//! raises SIGILL. Tach freezes one provider per timing contract per mode
//! instead of running a startup tournament.
//!
//! `Instant` reads the bare architectural counter `CNTVCT_EL0` where the mode
//! permits it (`SPEC`/`NOSPEC_APPLE`), scaled by `CNTFRQ_EL0` — 24 MHz on
//! M1/M2, 1 GHz on M3/M4. Per ADR-0005 the bare read satisfies the same-thread
//! monotonic wall-rate contract (frozen at 0 violations across ~2.8e9 paired
//! reads on M1 Max and M4 Pro) at a fraction of the XNU protocol cost, and on
//! Apple Silicon advances through system sleep exactly like both Mach
//! timelines. It is its own tick domain, so the `Instant` scale follows the
//! same mode gate. Where no bare read is permitted, `Instant` uses the
//! self-synchronizing commpage offset (`NOSPEC`) or `mach_absolute_time`
//! (`NONE`), both in the Mach-timebase domain.
//!
//! `OrderedInstant` is a correctness-gated pick, not a speed tournament: the
//! mode names the self-synchronizing register XNU permits — Apple `ACNTVCT_EL0`
//! (`NOSPEC_APPLE`) or ARMv8.6 `CNTVCTSS_EL0` (`NOSPEC`) — otherwise an explicit
//! `isb sy; cntvct` barrier (`SPEC`) or `mach_absolute_time` (`NONE`) carries
//! the happens-before edge. An unbarriered read is never synchronization
//! ordered, so the bare counter is never an `OrderedInstant` pick. Every
//! ordered pick stays in the Mach-timebase domain.

use core::arch::asm;
use core::sync::atomic::{AtomicU8, Ordering};

const COMM_PAGE_BASE: usize = 0x0000_000f_ffff_c000;
const TIMEBASE_OFFSET: usize = COMM_PAGE_BASE + 0x088;
const USER_TIMEBASE: usize = COMM_PAGE_BASE + 0x090;

#[cfg(test)]
const USER_TIMEBASE_NONE: u8 = 0;
const USER_TIMEBASE_SPEC: u8 = 1;
const USER_TIMEBASE_NOSPEC: u8 = 2;
const USER_TIMEBASE_NOSPEC_APPLE: u8 = 3;

// MODE_UNREAD sentinel is outside the valid 0..=3 range.
const MODE_UNREAD: u8 = 0xFF;
static CACHED_MODE: AtomicU8 = AtomicU8::new(MODE_UNREAD);

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  match resolved_mode() {
    // Modes 1/3 permit the bare `CNTVCT_EL0`; it is its own `CNTFRQ` domain.
    USER_TIMEBASE_SPEC | USER_TIMEBASE_NOSPEC_APPLE => bare_cntvct(),
    // Mode 2 permits the self-synchronizing register; Mach-timebase domain.
    USER_TIMEBASE_NOSPEC => cntvctss_absolute_time(),
    // Mode 0/NONE (and any malformed byte) permits no EL0 counter read.
    _ => mach_absolute(),
  }
}

// OrderedInstant is a CORRECTNESS capability gate, not a deleted speed
// tournament — do not remove it (mirrors the x86 LFENCE gate). Reading an EL0
// system register the commpage mode does not permit raises SIGILL, so the mode
// byte decides which self-synchronizing register XNU exposes: Apple's
// `ACNTVCT_EL0` in NOSPEC_APPLE, ARMv8.6 `CNTVCTSS_EL0` in NOSPEC. Where the
// mode exposes no self-sync register, the explicit `isb sy; cntvct` barrier
// (SPEC) or `mach_absolute_time` (NONE, which permits no EL0 counter at all)
// carries the happens-before edge instead. A bare unbarriered `CNTVCT_EL0` read
// is NEVER an ordered candidate (ADR-0005): it orders CNTVCT reads against one
// another, but not against surrounding work.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  match resolved_mode() {
    USER_TIMEBASE_NOSPEC_APPLE => acntvct_absolute_time(),
    USER_TIMEBASE_NOSPEC => cntvctss_absolute_time(),
    USER_TIMEBASE_SPEC => cntvct_ordered_absolute_time(),
    _ => mach_absolute(),
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unordered() -> u64 {
  match resolved_mode() {
    // The self-sync reads have no separable barrier: identical to ordered.
    USER_TIMEBASE_NOSPEC_APPLE => acntvct_absolute_time(),
    USER_TIMEBASE_NOSPEC => cntvctss_absolute_time(),
    // Mode 1's ordered pick minus the explicit `isb`: plain `cntvct`, NO barrier.
    USER_TIMEBASE_SPEC => cntvct_absolute_time(),
    _ => mach_absolute(),
  }
}

/// Resolve the process-immutable commpage timebase mode, caching it after the
/// first read. Racing threads compute the same immutable mode, so the relaxed
/// load/store is sufficient (no CAS).
#[inline(always)]
#[allow(clippy::inline_always)]
fn resolved_mode() -> u8 {
  let cached = CACHED_MODE.load(Ordering::Relaxed);
  if cached != MODE_UNREAD {
    return cached;
  }
  let mode = user_timebase_mode();
  CACHED_MODE.store(mode, Ordering::Relaxed);
  mode
}

// The bare counter is its own tick domain: `CNTFRQ_EL0` reports 24 MHz on
// M1/M2 and 1 GHz on M3/M4, while every XNU protocol route stays in
// Mach-timebase ticks. The `Instant` READ and this SCALE therefore branch on
// the same mode predicate: modes 1/3 read bare and scale by `CNTFRQ`; every
// other mode reads a Mach-timebase source and scales by the Mach ratio. A
// mismatch would make `elapsed()` wrong by ~40x on M3/M4 (1 GHz vs 24 MHz).
#[inline]
pub(crate) fn instant_nanos_per_tick_q32() -> u64 {
  let mode = resolved_mode();
  if mode == USER_TIMEBASE_SPEC || mode == USER_TIMEBASE_NOSPEC_APPLE {
    crate::arch::scale_from_ratio(1_000_000_000, cntfrq())
  } else {
    mach_nanos_per_tick_q32()
  }
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn bare_cntvct() -> u64 {
  let counter: u64;
  // SAFETY: `mrs cntvct_el0` reads the architectural virtual counter and touches
  // no memory or stack. Unordered by contract: `Instant` samples carry no
  // synchronization edge, and same-thread monotonicity of the bare read is
  // frozen at 0 violations across ~2.8e9 paired reads (M1 Max, M4 Pro).
  unsafe {
    asm!("mrs {}, cntvct_el0", out(reg) counter, options(nostack, nomem, preserves_flags));
  }
  counter
}

#[inline]
fn cntfrq() -> u64 {
  let frequency: u64;
  // SAFETY: `mrs cntfrq_el0` reads the architectural counter-frequency register
  // and touches no memory or stack. Bits [31:0] carry the rate in Hz.
  unsafe {
    asm!("mrs {}, cntfrq_el0", out(reg) frequency, options(nostack, nomem, preserves_flags));
  }
  frequency & 0xffff_ffff
}

#[inline]
fn mach_nanos_per_tick_q32() -> u64 {
  let (numer, denom) = super::fallback::mach_timebase();
  crate::arch::scale_from_ratio(u64::from(numer), u64::from(denom))
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn mach_absolute() -> u64 {
  super::fallback::mach_time()
}

// REMOVE-CRUFT EXCEPTION: `mach_continuous` (and its extern) has no fixed-pick
// consumer, but the `benches/apple_suspend_probe.rs` `[[bench]]` target reads
// it through `bench_exact_mach_continuous` to record `Instant`'s suspend
// semantic (ADR-0005 §5.1d). It is retained as bench/EVID-only.
#[cfg(feature = "bench-internal")]
unsafe extern "C" {
  fn mach_continuous_time() -> u64;
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(clippy::inline_always)]
fn mach_continuous() -> u64 {
  // SAFETY: `mach_continuous_time` takes no arguments and returns an exact Mach tick value.
  unsafe { mach_continuous_time() }
}

// The commpage timebase byte is the SIGILL gate input: XNU sets it at `exec`
// and every `fork` child inherits it unchanged, so a process-lifetime cache is
// exact. `0=NONE` (no EL0 counter), `1=SPEC`, `2=NOSPEC`, `3=NOSPEC_APPLE`.
#[inline(always)]
#[allow(clippy::inline_always)]
fn user_timebase_mode() -> u8 {
  // SAFETY: XNU maps the kernel-owned read-only commpage at this fixed arm64 address.
  unsafe { core::ptr::read_volatile(USER_TIMEBASE as *const u8) }
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn cntvct_absolute_time() -> u64 {
  let result: u64;
  // SAFETY: mode 1 permits `CNTVCT_EL0`; the offset retry is XNU's wake correction protocol.
  unsafe {
    asm!(
      "2:",
      "ldr {before}, [{offset}]",
      "mrs {counter}, cntvct_el0",
      "ldr {after}, [{offset}]",
      "cmp {before}, {after}",
      "b.ne 2b",
      "add {result}, {counter}, {before}",
      offset = in(reg) TIMEBASE_OFFSET,
      before = out(reg) _,
      counter = out(reg) _,
      after = out(reg) _,
      result = lateout(reg) result,
      options(nostack),
    );
  }
  result
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn cntvct_ordered_absolute_time() -> u64 {
  let result: u64;
  // SAFETY: mode 1 permits the counter; `isb` orders the sample after preceding work.
  unsafe {
    asm!(
      "isb sy",
      "2:",
      "ldr {before}, [{offset}]",
      "mrs {counter}, cntvct_el0",
      "ldr {after}, [{offset}]",
      "cmp {before}, {after}",
      "b.ne 2b",
      "add {result}, {counter}, {before}",
      offset = in(reg) TIMEBASE_OFFSET,
      before = out(reg) _,
      counter = out(reg) _,
      after = out(reg) _,
      result = lateout(reg) result,
      options(nostack),
    );
  }
  result
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn cntvctss_absolute_time() -> u64 {
  let result: u64;
  // SAFETY: mode 2 permits the self-synchronizing `CNTVCTSS_EL0` register.
  unsafe {
    asm!(
      "2:",
      "ldr {before}, [{offset}]",
      "mrs {counter}, S3_3_C14_C0_6",
      "ldr {after}, [{offset}]",
      "cmp {before}, {after}",
      "b.ne 2b",
      "add {result}, {counter}, {before}",
      offset = in(reg) TIMEBASE_OFFSET,
      before = out(reg) _,
      counter = out(reg) _,
      after = out(reg) _,
      result = lateout(reg) result,
      options(nostack),
    );
  }
  result
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn acntvct_absolute_time() -> u64 {
  let result: u64;
  // SAFETY: mode 3 permits Apple's self-synchronizing `ACNTVCT_EL0` register.
  unsafe {
    asm!(
      "2:",
      "ldr {before}, [{offset}]",
      "mrs {counter}, S3_4_C15_C10_6",
      "ldr {after}, [{offset}]",
      "cmp {before}, {after}",
      "b.ne 2b",
      "add {result}, {counter}, {before}",
      offset = in(reg) TIMEBASE_OFFSET,
      before = out(reg) _,
      counter = out(reg) _,
      after = out(reg) _,
      result = lateout(reg) result,
      options(nostack),
    );
  }
  result
}

#[cfg(feature = "bench-internal")]
const fn provider_name<const ORDERED: bool>(mode: u8) -> &'static str {
  match mode {
    USER_TIMEBASE_SPEC if ORDERED => "apple_commpage_isb_cntvct_offset",
    USER_TIMEBASE_SPEC => "apple_bare_cntvct",
    USER_TIMEBASE_NOSPEC => "apple_commpage_cntvctss_offset",
    USER_TIMEBASE_NOSPEC_APPLE if ORDERED => "apple_commpage_acntvct_offset",
    USER_TIMEBASE_NOSPEC_APPLE => "apple_bare_cntvct",
    _ => "apple_mach_absolute_time",
  }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_instant_provider() -> &'static str {
  provider_name::<false>(resolved_mode())
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_provider() -> &'static str {
  provider_name::<true>(resolved_mode())
}

#[cfg(feature = "bench-internal")]
#[derive(Clone, Copy)]
#[allow(dead_code)] // Consumed by the benchmark harness outside this module.
pub(crate) struct BenchPrimitive {
  pub(crate) name: &'static str,
  pub(crate) read: fn() -> u64,
  pub(crate) nanos_per_tick_q32: u64,
}

#[cfg(feature = "bench-internal")]
#[inline]
fn bench_nanos_per_tick_q32() -> u64 {
  mach_nanos_per_tick_q32()
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The benchmark harness selects once before its timing loop.
pub(crate) fn bench_selected_instant_primitive() -> BenchPrimitive {
  instant_bench_primitive(resolved_mode())
}

#[cfg(feature = "bench-internal")]
#[allow(dead_code)] // The benchmark harness selects once before its timing loop.
pub(crate) fn bench_selected_ordered_primitive() -> BenchPrimitive {
  ordered_bench_primitive(resolved_mode())
}

#[cfg(feature = "bench-internal")]
fn instant_bench_primitive(mode: u8) -> BenchPrimitive {
  let read = match mode {
    USER_TIMEBASE_SPEC | USER_TIMEBASE_NOSPEC_APPLE => bare_cntvct as fn() -> u64,
    USER_TIMEBASE_NOSPEC => cntvctss_absolute_time,
    _ => mach_absolute,
  };
  let nanos_per_tick_q32 = if mode == USER_TIMEBASE_SPEC || mode == USER_TIMEBASE_NOSPEC_APPLE {
    crate::arch::scale_from_ratio(1_000_000_000, cntfrq())
  } else {
    bench_nanos_per_tick_q32()
  };
  BenchPrimitive { name: provider_name::<false>(mode), read, nanos_per_tick_q32 }
}

#[cfg(feature = "bench-internal")]
fn ordered_bench_primitive(mode: u8) -> BenchPrimitive {
  let read = match mode {
    USER_TIMEBASE_NOSPEC_APPLE => acntvct_absolute_time as fn() -> u64,
    USER_TIMEBASE_NOSPEC => cntvctss_absolute_time,
    USER_TIMEBASE_SPEC => cntvct_ordered_absolute_time,
    _ => mach_absolute,
  };
  BenchPrimitive {
    name: provider_name::<true>(mode),
    read,
    nanos_per_tick_q32: bench_nanos_per_tick_q32(),
  }
}

#[cfg(feature = "bench-internal")]
macro_rules! exact_bench_reader {
  ($name:ident, $reader:ident) => {
    #[inline(always)]
    #[allow(dead_code)] // The benchmark harness calls each eligible reader directly.
    pub(crate) fn $name() -> u64 {
      $reader()
    }
  };
}

#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_bare_cntvct, bare_cntvct);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_mach_absolute, mach_absolute);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_mach_continuous, mach_continuous);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_cntvct_ordered_absolute, cntvct_ordered_absolute_time);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_cntvctss_absolute, cntvctss_absolute_time);
#[cfg(feature = "bench-internal")]
exact_bench_reader!(bench_exact_acntvct_absolute, acntvct_absolute_time);

#[cfg(test)]
mod tests {
  use super::*;

  // ESC-APPLE-ORDERED-SELECTION probe: does each candidate ordered provider honor
  // the OrderedInstant happens-before contract on this hardware? Each thread
  // Acquire-loads the max published raw tick, reads the candidate, and counts a
  // violation if its read is < the observed tick. Same-provider raw ticks share
  // one monotonic domain, so the comparison is valid. Bare CNTVCT is the negative
  // control (unbarriered → must show violations, proving the harness detects
  // failures); isb+cntvct is the positive control (must show 0). This decides
  // whether a cheaper-than-isb provider (mach_absolute / self-sync ACNTVCT) is an
  // eligible ordered pick (~5 ns) or whether only the barriered route (~10 ns) is
  // correct. Run: cargo test --lib ordered_candidate_happens_before_survey -- --nocapture
  #[test]
  #[ignore = "cross-thread stress survey; run explicitly for the Apple ordered ruling"]
  fn ordered_candidate_happens_before_survey() {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as O};
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::vec::Vec;

    type OrderedReader = fn() -> u64;
    let candidates: [(&str, OrderedReader); 4] = [
      ("bare_cntvct        (unbarriered control)", bare_cntvct),
      ("mach_absolute      (current runtime pick)", mach_absolute),
      ("acntvct_absolute   (self-sync register)", acntvct_absolute_time),
      ("isb+cntvct         (barriered control)", cntvct_ordered_absolute_time),
    ];
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
    let secs: u64 =
      std::env::var("TACH_SURVEY_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(2);
    std::eprintln!("SURVEY threads={threads}, {secs}s/candidate");
    for (name, reader) in candidates {
      let published = Arc::new(AtomicU64::new(0));
      let stop = Arc::new(AtomicBool::new(false));
      let gate = Arc::new(Barrier::new(threads + 1));
      let handles: Vec<_> = (0..threads)
        .map(|_| {
          let published = Arc::clone(&published);
          let stop = Arc::clone(&stop);
          let gate = Arc::clone(&gate);
          thread::spawn(move || {
            let (mut violations, mut reads) = (0_u64, 0_u64);
            gate.wait();
            while !stop.load(O::Relaxed) {
              let observed = published.load(O::Acquire);
              let now = reader();
              reads += 1;
              if now < observed {
                violations += 1;
              }
              published.fetch_max(now, O::Release);
            }
            (violations, reads)
          })
        })
        .collect();
      gate.wait();
      thread::sleep(std::time::Duration::from_secs(secs));
      stop.store(true, O::Relaxed);
      let (v, r) = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .fold((0_u64, 0_u64), |(av, ar), (v, r)| (av + v, ar + r));
      std::eprintln!("SURVEY {name}: {v} violations / {r} reads");
    }
  }

  #[test]
  fn bare_counter_is_instant_only_and_scaled_by_cntfrq() {
    let mode = resolved_mode();
    if mode == USER_TIMEBASE_SPEC || mode == USER_TIMEBASE_NOSPEC_APPLE {
      let hz = cntfrq();
      assert!(hz >= 1_000_000, "implausible cntfrq: {hz}");
      assert_eq!(instant_nanos_per_tick_q32(), crate::arch::scale_from_ratio(1_000_000_000, hz));
      let mut previous = bare_cntvct();
      for _ in 0..100_000 {
        let current = bare_cntvct();
        assert!(current >= previous, "bare counter moved backward on one thread");
        previous = current;
      }
    } else {
      assert_eq!(instant_nanos_per_tick_q32(), mach_nanos_per_tick_q32());
    }
    // Bare CNTVCT is never the ordered pick: OrderedInstant reads a self-sync
    // register, an explicit isb barrier, or mach_absolute — all in the
    // Mach-timebase domain, so a mach_absolute bracket contains the ordered
    // read while an out-of-domain bare read (`cntvct` < `cntvct + offset`)
    // could not.
    let before = mach_absolute();
    let ordered = ticks_ordered();
    let after = mach_absolute();
    assert!(before <= ordered && ordered <= after, "ordered read left the mach-timebase domain");
  }

  #[test]
  fn ordered_unordered_reads_remain_in_the_ordered_domain() {
    let ordered_before = ticks_ordered();
    let unordered = ticks_ordered_unordered();
    let ordered_after = ticks_ordered();
    assert!(ordered_before <= unordered && unordered <= ordered_after);
  }

  #[test]
  fn direct_protocols_share_their_xnu_timelines() {
    for _ in 0..10_000 {
      let before = mach_absolute();
      let direct = ticks_ordered();
      let after = mach_absolute();
      assert!(before <= direct && direct <= after);
    }
  }

  #[test]
  fn selected_protocols_are_monotonic() {
    let mut instant = ticks();
    let mut ordered = ticks_ordered();
    for _ in 0..100_000 {
      let next_instant = ticks();
      let next_ordered = ticks_ordered();
      assert!(next_instant >= instant);
      assert!(next_ordered >= ordered);
      instant = next_instant;
      ordered = next_ordered;
    }
  }

  #[test]
  fn resolved_gate_only_reads_registers_the_commpage_permits() {
    // Reading an EL0 counter register the commpage mode forbids raises SIGILL.
    // The mode is process-fixed, so exercising all three dispatched reads proves
    // the gate never emits a forbidden register: a wrong arm would trap here
    // instead of returning.
    let mode = user_timebase_mode();
    assert_eq!(resolved_mode(), mode);
    let _ = ticks();
    let _ = ticks_ordered();
    let _ = ticks_ordered_unordered();
    if mode == USER_TIMEBASE_SPEC || mode == USER_TIMEBASE_NOSPEC_APPLE {
      // Bare CNTVCT_EL0 is permitted; Instant reads it and CNTFRQ scales it.
      assert!(cntfrq() >= 1_000_000);
      assert_eq!(
        instant_nanos_per_tick_q32(),
        crate::arch::scale_from_ratio(1_000_000_000, cntfrq())
      );
    } else {
      // NOSPEC self-sync or NONE (no EL0 counter): Instant stays Mach-domain.
      assert_eq!(instant_nanos_per_tick_q32(), mach_nanos_per_tick_q32());
      if mode == USER_TIMEBASE_NONE {
        // Mode 0 permits no EL0 counter at all; every contract is mach_absolute.
        let before = mach_absolute();
        let sample = ticks();
        let after = mach_absolute();
        assert!(before <= sample && sample <= after);
      }
    }
  }

  #[test]
  fn selected_providers_survive_fork() {
    let mode_before = resolved_mode();
    let instant_before = ticks();
    let ordered_before = ticks_ordered();
    // SAFETY: the child performs only clock reads and `_exit`; the parent waits for it.
    let child = unsafe { libc::fork() };
    assert!(child >= 0);
    if child == 0 {
      // The mode is inherited across fork, so the child reads the same fixed pick.
      let ok = resolved_mode() == mode_before
        && user_timebase_mode() == mode_before
        && ticks() >= instant_before
        && ticks_ordered() >= ordered_before;
      // SAFETY: `_exit` terminates the child without inherited Rust cleanup.
      unsafe { libc::_exit(if ok { 0 } else { 1 }) };
    }

    let mut status = 0;
    // SAFETY: `status` is writable and `child` identifies this process's live child.
    assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
    assert_eq!(status, 0);
  }
}
