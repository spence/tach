//! JavaScript-hosted clocks for wasm32 unknown/none targets.
//!
//! Initialization measures every eligible monotonic host clock through the
//! same guarded JavaScript path used by steady-state reads. The selected clock
//! never changes domains: a later host failure freezes its last value and
//! changes provider introspection to `Unavailable`.

use core::sync::atomic::{AtomicU64, Ordering};

use wasm_bindgen::prelude::wasm_bindgen;

use crate::{ThreadCpuProvider, ThreadCpuReadCost};

const PROVIDER_PERFORMANCE_NOW: u32 = 1;
const PROVIDER_NODE_HRTIME: u32 = 2;

static ORDERED_MAX: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internal")]
static BENCH_ORDERED_PERFORMANCE_MAX: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internal")]
static BENCH_ORDERED_HRTIME_MAX: AtomicU64 = AtomicU64::new(0);

#[wasm_bindgen(inline_js = r#"
const TACH_UNAVAILABLE = 0;
const TACH_PERFORMANCE_NOW = 1;
const TACH_NODE_HRTIME = 2;
const TACH_READS = 4096;
const TACH_WARMUP_READS = 65536;
const TACH_SAMPLES = 9;
const TACH_REQUIRED_WINS = 8;
let tachSink = 0;

function tachTryMillis(read) {
  if (read === null) {
    return null;
  }
  try {
    const value = read();
    return Number.isFinite(value) && value >= 0 ? value : null;
  } catch (_) {
    return null;
  }
}

function tachTryTimer(timer) {
  if (timer === null) {
    return null;
  }
  try {
    const value = timer();
    return typeof value === "bigint" && value >= 0n ? value : null;
  } catch (_) {
    return null;
  }
}

function tachEligible(read) {
  const first = tachTryMillis(read);
  const second = tachTryMillis(read);
  return first !== null && second !== null && second >= first;
}

function tachWarmUp(read) {
  let previous = 0;
  let sink = 0;
  for (let index = 0; index < TACH_WARMUP_READS; index += 1) {
    const value = tachTryMillis(read);
    if (value === null || value < previous) {
      return false;
    }
    previous = value;
    sink += value;
  }
  tachSink = sink;
  return true;
}

function tachMeasure(read, timer) {
  const start = tachTryTimer(timer);
  if (start === null) {
    return { elapsed: null, sourceFailed: false, timerFailed: true };
  }
  let previous = 0;
  let sink = 0;
  for (let index = 0; index < TACH_READS; index += 1) {
    const value = tachTryMillis(read);
    if (value === null || value < previous) {
      return { elapsed: null, sourceFailed: true, timerFailed: false };
    }
    previous = value;
    sink += value;
  }
  const end = tachTryTimer(timer);
  if (end === null || end < start) {
    return { elapsed: null, sourceFailed: false, timerFailed: true };
  }
  tachSink = sink;
  return { elapsed: end - start, sourceFailed: false, timerFailed: false };
}

function tachMedian(samples) {
  const sorted = samples.slice().sort((left, right) => left < right ? -1 : left > right ? 1 : 0);
  return sorted[Math.floor(sorted.length / 2)];
}

function tachSelection(provider, read, performanceEligible, hrtimeEligible) {
  return {
    provider,
    read,
    performanceEligible,
    hrtimeEligible,
    performanceMedian: 0n,
    hrtimeMedian: 0n,
    performanceSamples: [],
    hrtimeSamples: [],
    allowance: 0n,
    decisiveWins: 0,
  };
}

function tachCommit(selection, ordered) {
  if (selection.provider === TACH_PERFORMANCE_NOW) {
    selection.read = ordered
      ? tachGuardPerformanceOrdered(selection)
      : tachGuardPerformanceLocal(selection);
  } else if (selection.provider === TACH_NODE_HRTIME) {
    selection.read = ordered
      ? tachGuardHrtimeOrdered(selection)
      : tachGuardHrtimeLocal(selection);
  } else {
    selection.read = () => 0;
  }
  return selection;
}

function tachSelect(performanceRead, hrtimeRead, timer, ordered) {
  let performanceEligible = tachEligible(performanceRead);
  let hrtimeEligible = tachEligible(hrtimeRead);

  if (performanceEligible && hrtimeEligible) {
    if (!tachWarmUp(performanceRead)) {
      performanceEligible = false;
    }
    if (!tachWarmUp(hrtimeRead)) {
      hrtimeEligible = false;
    }
  }

  const performanceSamples = [];
  const hrtimeSamples = [];
  if (performanceEligible && hrtimeEligible) {
    for (let sample = 0; sample < TACH_SAMPLES; sample += 1) {
      const performanceFirst = (sample & 1) === 0;
      const first = tachMeasure(performanceFirst ? performanceRead : hrtimeRead, timer);
      const second = tachMeasure(performanceFirst ? hrtimeRead : performanceRead, timer);
      const performance = performanceFirst ? first : second;
      const hrtime = performanceFirst ? second : first;

      if (performance.sourceFailed) {
        performanceEligible = false;
      }
      if (hrtime.sourceFailed) {
        hrtimeEligible = false;
      }
      if (performance.timerFailed || hrtime.timerFailed) {
        hrtimeEligible = false;
      }
      if (!performanceEligible || !hrtimeEligible) {
        break;
      }
      performanceSamples.push(performance.elapsed);
      hrtimeSamples.push(hrtime.elapsed);
    }
  }

  if (!performanceEligible) {
    return tachCommit(
      hrtimeEligible
        ? tachSelection(TACH_NODE_HRTIME, hrtimeRead, false, true)
        : tachSelection(TACH_UNAVAILABLE, () => 0, false, false),
      ordered,
    );
  }
  if (!hrtimeEligible || performanceSamples.length !== TACH_SAMPLES) {
    return tachCommit(
      tachSelection(TACH_PERFORMANCE_NOW, performanceRead, true, false),
      ordered,
    );
  }

  const performanceMedian = tachMedian(performanceSamples);
  const hrtimeMedian = tachMedian(hrtimeSamples);
  const allowance = performanceMedian / 20n > BigInt(TACH_READS)
    ? performanceMedian / 20n
    : BigInt(TACH_READS);
  let decisiveWins = 0;
  for (let sample = 0; sample < TACH_SAMPLES; sample += 1) {
    if (hrtimeSamples[sample] + allowance < performanceSamples[sample]) {
      decisiveWins += 1;
    }
  }
  const hrtimeSelected =
    decisiveWins >= TACH_REQUIRED_WINS && hrtimeMedian + allowance < performanceMedian;
  const selection = tachSelection(
    hrtimeSelected ? TACH_NODE_HRTIME : TACH_PERFORMANCE_NOW,
    hrtimeSelected ? hrtimeRead : performanceRead,
    true,
    true,
  );
  selection.performanceMedian = performanceMedian;
  selection.hrtimeMedian = hrtimeMedian;
  selection.performanceSamples = performanceSamples;
  selection.hrtimeSamples = hrtimeSamples;
  selection.allowance = allowance;
  selection.decisiveWins = decisiveWins;
  return tachCommit(selection, ordered);
}

let tachDateAttempted = false;
let tachDateSample = null;
function tachDateEpochOnce() {
  if (tachDateAttempted) {
    return tachDateSample;
  }
  tachDateAttempted = true;
  try {
    const date = globalThis.Date;
    if (date !== undefined && date !== null && typeof date.now === "function") {
      const sample = date.now();
      if (Number.isFinite(sample)) {
        tachDateSample = sample;
      }
    }
  } catch (_) {
    tachDateSample = null;
  }
  return tachDateSample;
}

let tachPerformanceNow = null;
let tachPerformanceOriginMillis = null;
let tachPerformanceLocal = null;
let tachPerformanceOrdered = null;
try {
  const performance = globalThis.performance;
  if (performance !== undefined && performance !== null && typeof performance.now === "function") {
    const now = performance.now.bind(performance);
    tachPerformanceNow = now;
    tachPerformanceLocal = () => now();

    let originMillis = Number.isFinite(performance.timeOrigin) ? performance.timeOrigin : null;
    if (originMillis === null) {
      const localMillis = tachTryMillis(tachPerformanceLocal);
      const epochMillis = tachDateEpochOnce();
      if (localMillis !== null && epochMillis !== null) {
        originMillis = epochMillis - localMillis;
      }
    }
    if (originMillis !== null) {
      tachPerformanceOriginMillis = originMillis;
      tachPerformanceOrdered = () => originMillis + now();
    }
  }
} catch (_) {
  tachPerformanceNow = null;
  tachPerformanceOriginMillis = null;
  tachPerformanceLocal = null;
  tachPerformanceOrdered = null;
}

let tachHrtimeBigint = null;
let tachHrtimeTimer = null;
let tachHrtimeLocalBase = 0n;
let tachHrtimeOrderedBase = 0n;
let tachHrtimeOrderedOriginMillis = 0;
let tachHrtimeLocal = null;
let tachHrtimeOrdered = null;
try {
  const process = globalThis.process;
  const hrtime = process === undefined || process === null ? null : process.hrtime;
  if (typeof hrtime === "function" && typeof hrtime.bigint === "function") {
    const bigint = hrtime.bigint.bind(hrtime);
    tachHrtimeBigint = bigint;
    tachHrtimeTimer = () => bigint();

    const localBase = bigint();
    tachHrtimeLocalBase = localBase;
    tachHrtimeLocal = () => Number(bigint() - localBase) / 1000000;

    const orderedBase = bigint();
    let orderedOrigin = tachTryMillis(tachPerformanceOrdered);
    if (orderedOrigin === null) {
      orderedOrigin = Number(orderedBase) / 1000000;
    }
    tachHrtimeOrderedBase = orderedBase;
    tachHrtimeOrderedOriginMillis = orderedOrigin;
    tachHrtimeOrdered = () => orderedOrigin + Number(bigint() - orderedBase) / 1000000;
  }
} catch (_) {
  tachHrtimeBigint = null;
  tachHrtimeTimer = null;
  tachHrtimeLocal = null;
  tachHrtimeOrdered = null;
}

function tachUnavailable(selection, last) {
  if (selection !== null) {
    selection.provider = TACH_UNAVAILABLE;
  }
  return last;
}

function tachGuardPerformanceLocal(selection) {
  let last = 0;
  let unavailable = tachPerformanceNow === null;
  return () => {
    if (unavailable) {
      return last;
    }
    try {
      const value = tachPerformanceNow();
      if (!Number.isFinite(value) || value < last) {
        unavailable = true;
        return tachUnavailable(selection, last);
      }
      last = value;
      return value;
    } catch (_) {
      unavailable = true;
      return tachUnavailable(selection, last);
    }
  };
}

function tachGuardPerformanceOrdered(selection) {
  let last = 0;
  let unavailable = tachPerformanceNow === null || tachPerformanceOriginMillis === null;
  return () => {
    if (unavailable) {
      return last;
    }
    try {
      const value = tachPerformanceOriginMillis + tachPerformanceNow();
      if (!Number.isFinite(value) || value < last) {
        unavailable = true;
        return tachUnavailable(selection, last);
      }
      last = value;
      return value;
    } catch (_) {
      unavailable = true;
      return tachUnavailable(selection, last);
    }
  };
}

function tachGuardHrtimeLocal(selection) {
  let last = 0;
  let unavailable = tachHrtimeBigint === null;
  return () => {
    if (unavailable) {
      return last;
    }
    try {
      const value = Number(tachHrtimeBigint() - tachHrtimeLocalBase) / 1000000;
      if (!Number.isFinite(value) || value < last) {
        unavailable = true;
        return tachUnavailable(selection, last);
      }
      last = value;
      return value;
    } catch (_) {
      unavailable = true;
      return tachUnavailable(selection, last);
    }
  };
}

function tachGuardHrtimeOrdered(selection) {
  let last = 0;
  let unavailable = tachHrtimeBigint === null;
  return () => {
    if (unavailable) {
      return last;
    }
    try {
      const value = tachHrtimeOrderedOriginMillis
        + Number(tachHrtimeBigint() - tachHrtimeOrderedBase) / 1000000;
      if (!Number.isFinite(value) || value < last) {
        unavailable = true;
        return tachUnavailable(selection, last);
      }
      last = value;
      return value;
    } catch (_) {
      unavailable = true;
      return tachUnavailable(selection, last);
    }
  };
}

const tachLocalSelection = tachSelect(
  tachPerformanceLocal,
  tachHrtimeLocal,
  tachHrtimeTimer,
  false,
);
const tachOrderedSelection = tachSelect(
  tachPerformanceOrdered,
  tachHrtimeOrdered,
  tachHrtimeTimer,
  true,
);
const tachExactPerformanceLocal = tachGuardPerformanceLocal(null);
const tachExactHrtimeLocal = tachGuardHrtimeLocal(null);
const tachExactPerformanceOrdered = tachGuardPerformanceOrdered(null);
const tachExactHrtimeOrdered = tachGuardHrtimeOrdered(null);

export function tachWallNowMillis() {
  return tachLocalSelection.read();
}

export function tachOrderedWallNowMillis() {
  return tachOrderedSelection.read();
}

export function tachWallProvider() {
  return tachLocalSelection.provider;
}

export function tachOrderedWallProvider() {
  return tachOrderedSelection.provider;
}

export function tachExactPerformanceNowMillis() {
  return tachExactPerformanceLocal();
}

export function tachExactHrtimeNowMillis() {
  return tachExactHrtimeLocal();
}

export function tachWallPerformanceMedianNanos() {
  return Number(tachLocalSelection.performanceMedian);
}

export function tachWallHrtimeMedianNanos() {
  return Number(tachLocalSelection.hrtimeMedian);
}

export function tachWallAllowanceNanos() {
  return Number(tachLocalSelection.allowance);
}

export function tachWallHrtimeDecisiveWins() {
  return tachLocalSelection.decisiveWins;
}

export function tachWallPerformanceSampleNanos(index) {
  return Number(tachLocalSelection.performanceSamples[index] ?? 0n);
}

export function tachWallHrtimeSampleNanos(index) {
  return Number(tachLocalSelection.hrtimeSamples[index] ?? 0n);
}

export function tachOrderedExactPerformanceNowMillis() {
  return tachExactPerformanceOrdered();
}

export function tachOrderedExactHrtimeNowMillis() {
  return tachExactHrtimeOrdered();
}

export function tachOrderedWallPerformanceMedianNanos() {
  return Number(tachOrderedSelection.performanceMedian);
}

export function tachOrderedWallHrtimeMedianNanos() {
  return Number(tachOrderedSelection.hrtimeMedian);
}

export function tachOrderedWallAllowanceNanos() {
  return Number(tachOrderedSelection.allowance);
}

export function tachOrderedWallHrtimeDecisiveWins() {
  return tachOrderedSelection.decisiveWins;
}

export function tachOrderedWallPerformanceSampleNanos(index) {
  return Number(tachOrderedSelection.performanceSamples[index] ?? 0n);
}

export function tachOrderedWallHrtimeSampleNanos(index) {
  return Number(tachOrderedSelection.hrtimeSamples[index] ?? 0n);
}

export function tachNodeThreadCpuUsage() {
  try {
    const process = globalThis.process;
    if (process === undefined || typeof process.threadCpuUsage !== "function") {
      return -1;
    }
    const usage = process.threadCpuUsage();
    const micros = Number(usage.user) + Number(usage.system);
    return Number.isFinite(micros) && micros >= 0 ? micros : -1;
  } catch (_) {
    return -1;
  }
}
"#)]
unsafe extern "C" {
  #[wasm_bindgen(js_name = tachWallNowMillis)]
  fn wall_now_millis() -> f64;
  #[wasm_bindgen(js_name = tachOrderedWallNowMillis)]
  fn ordered_wall_now_millis() -> f64;
  #[wasm_bindgen(js_name = tachWallProvider)]
  fn wall_provider_id() -> u32;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachOrderedWallProvider)]
  fn ordered_wall_provider_id() -> u32;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachExactPerformanceNowMillis)]
  fn exact_performance_now_millis() -> f64;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachExactHrtimeNowMillis)]
  fn exact_hrtime_now_millis() -> f64;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachWallPerformanceMedianNanos)]
  fn wall_performance_median_nanos() -> f64;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachWallHrtimeMedianNanos)]
  fn wall_hrtime_median_nanos() -> f64;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachWallAllowanceNanos)]
  fn wall_allowance_nanos() -> f64;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachWallHrtimeDecisiveWins)]
  fn wall_hrtime_decisive_wins() -> u32;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachWallPerformanceSampleNanos)]
  fn wall_performance_sample_nanos(index: u32) -> f64;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachWallHrtimeSampleNanos)]
  fn wall_hrtime_sample_nanos(index: u32) -> f64;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachOrderedExactPerformanceNowMillis)]
  fn ordered_exact_performance_now_millis() -> f64;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachOrderedExactHrtimeNowMillis)]
  fn ordered_exact_hrtime_now_millis() -> f64;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachOrderedWallPerformanceMedianNanos)]
  fn ordered_wall_performance_median_nanos() -> f64;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachOrderedWallHrtimeMedianNanos)]
  fn ordered_wall_hrtime_median_nanos() -> f64;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachOrderedWallAllowanceNanos)]
  fn ordered_wall_allowance_nanos() -> f64;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachOrderedWallHrtimeDecisiveWins)]
  fn ordered_wall_hrtime_decisive_wins() -> u32;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachOrderedWallPerformanceSampleNanos)]
  fn ordered_wall_performance_sample_nanos(index: u32) -> f64;
  #[cfg(feature = "bench-internal")]
  #[wasm_bindgen(js_name = tachOrderedWallHrtimeSampleNanos)]
  fn ordered_wall_hrtime_sample_nanos(index: u32) -> f64;
  #[wasm_bindgen(js_name = tachNodeThreadCpuUsage)]
  fn node_thread_cpu_usage() -> f64;
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  millis_to_nanos(wall_now_millis())
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  let ticks = ticks_ordered_unclamped();
  ticks.max(ORDERED_MAX.fetch_max(ticks, Ordering::SeqCst))
}

#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unclamped() -> u64 {
  millis_to_nanos(ordered_wall_now_millis())
}

#[inline]
pub(crate) fn wall_provider() -> ThreadCpuProvider {
  provider_from_id(wall_provider_id())
}

#[inline]
pub(crate) fn wall_read_cost() -> ThreadCpuReadCost {
  match wall_provider() {
    ThreadCpuProvider::Unavailable => ThreadCpuReadCost::Unavailable,
    _ => ThreadCpuReadCost::HostCall,
  }
}

#[cfg(feature = "bench-internal")]
pub(crate) struct BenchWallSelectionEvidence {
  pub local_provider: ThreadCpuProvider,
  pub ordered_provider: ThreadCpuProvider,
  pub performance_median_ns: u64,
  pub hrtime_median_ns: u64,
  pub performance_batches_ns: [u64; 9],
  pub hrtime_batches_ns: [u64; 9],
  pub allowance_ns: u64,
  pub hrtime_decisive_wins: u32,
  pub ordered_performance_median_ns: u64,
  pub ordered_hrtime_median_ns: u64,
  pub ordered_performance_batches_ns: [u64; 9],
  pub ordered_hrtime_batches_ns: [u64; 9],
  pub ordered_allowance_ns: u64,
  pub ordered_hrtime_decisive_wins: u32,
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_selection_evidence() -> BenchWallSelectionEvidence {
  BenchWallSelectionEvidence {
    local_provider: wall_provider(),
    ordered_provider: provider_from_id(ordered_wall_provider_id()),
    performance_median_ns: wall_performance_median_nanos() as u64,
    hrtime_median_ns: wall_hrtime_median_nanos() as u64,
    performance_batches_ns: core::array::from_fn(|index| {
      wall_performance_sample_nanos(index as u32) as u64
    }),
    hrtime_batches_ns: core::array::from_fn(|index| {
      wall_hrtime_sample_nanos(index as u32) as u64
    }),
    allowance_ns: wall_allowance_nanos() as u64,
    hrtime_decisive_wins: wall_hrtime_decisive_wins(),
    ordered_performance_median_ns: ordered_wall_performance_median_nanos() as u64,
    ordered_hrtime_median_ns: ordered_wall_hrtime_median_nanos() as u64,
    ordered_performance_batches_ns: core::array::from_fn(|index| {
      ordered_wall_performance_sample_nanos(index as u32) as u64
    }),
    ordered_hrtime_batches_ns: core::array::from_fn(|index| {
      ordered_wall_hrtime_sample_nanos(index as u32) as u64
    }),
    ordered_allowance_ns: ordered_wall_allowance_nanos() as u64,
    ordered_hrtime_decisive_wins: ordered_wall_hrtime_decisive_wins(),
  }
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_exact_performance_ticks() -> u64 {
  millis_to_nanos(exact_performance_now_millis())
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_exact_hrtime_ticks() -> u64 {
  millis_to_nanos(exact_hrtime_now_millis())
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_exact_ordered_performance_ticks() -> u64 {
  let ticks = millis_to_nanos(ordered_exact_performance_now_millis());
  ticks.max(BENCH_ORDERED_PERFORMANCE_MAX.fetch_max(ticks, Ordering::SeqCst))
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_exact_ordered_hrtime_ticks() -> u64 {
  let ticks = millis_to_nanos(ordered_exact_hrtime_now_millis());
  ticks.max(BENCH_ORDERED_HRTIME_MAX.fetch_max(ticks, Ordering::SeqCst))
}

#[inline]
fn provider_from_id(provider: u32) -> ThreadCpuProvider {
  match provider {
    PROVIDER_PERFORMANCE_NOW => ThreadCpuProvider::PerformanceNow,
    PROVIDER_NODE_HRTIME => ThreadCpuProvider::NodeHrtime,
    _ => ThreadCpuProvider::Unavailable,
  }
}

#[inline]
fn millis_to_nanos(millis: f64) -> u64 {
  if !millis.is_finite() || millis < 0.0 {
    return 0;
  }
  (millis * 1_000_000.0) as u64
}

#[inline]
pub(crate) fn node_thread_cpu_usage_micros() -> Option<u64> {
  let micros = node_thread_cpu_usage();
  if !micros.is_finite() || micros < 0.0 {
    return None;
  }
  Some(micros as u64)
}
