//! Direct Emscripten host clocks.
//!
//! `Instant` measures Emscripten's guarded `performance.now()` path against
//! Node's guarded `process.hrtime.bigint()` path when both exist. A
//! non-threaded `GlobalInstant` uses that selected local timeline because no
//! value can cross a Rust thread boundary. With Emscripten pthread support
//! enabled, `GlobalInstant` measures two complete cross-thread-safe paths: an
//! epoch-correlated performance clock plus a shared atomic maximum, and
//! Emscripten's pthread-synchronized `emscripten_get_now`, aligned into that
//! epoch domain when both clocks are available, plus the same maximum.
//!
//! The selected provider is sticky. A later host exception or invalid value
//! freezes its last value and marks it unavailable instead of changing numeric
//! domains.

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
use core::sync::atomic::AtomicI64;
#[cfg(any(
  feature = "bench-internal",
  feature = "emscripten-pthreads",
  target_feature = "atomics",
))]
use core::sync::atomic::AtomicU64;
use core::sync::atomic::{AtomicU8, Ordering};

use crate::{ThreadCpuProvider, ThreadCpuReadCost};

const LOCAL_UNKNOWN: u8 = 0;
const LOCAL_SELECTING: u8 = 1;
const LOCAL_PERFORMANCE_NOW: u8 = 2;
const LOCAL_NODE_HRTIME: u8 = 3;
const LOCAL_UNAVAILABLE: u8 = 4;
const LOCAL_SELECTING_PERFORMANCE_NOW: u8 = 5;
const LOCAL_SELECTING_NODE_HRTIME: u8 = 6;
const LOCAL_SELECTING_UNAVAILABLE: u8 = 7;

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
const ORDERED_SELECTING: u8 = 1;
#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
const ORDERED_PERFORMANCE_EPOCH: u8 = 2;
#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
const ORDERED_EMSCRIPTEN_GET_NOW: u8 = 3;
#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
const ORDERED_UNAVAILABLE: u8 = 4;
#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
const ORDERED_LOCAL: u8 = 5;

static LOCAL_STATE: AtomicU8 = AtomicU8::new(LOCAL_UNKNOWN);
static PROBE_LOCAL_PROVIDER: AtomicU8 = AtomicU8::new(LOCAL_PERFORMANCE_NOW);

#[cfg(not(any(feature = "emscripten-pthreads", target_feature = "atomics")))]
static mut LOCAL_HOT_PROVIDER: u8 = LOCAL_UNKNOWN;

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
static ORDERED_STATE: AtomicU8 = AtomicU8::new(0);
#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
static ORDERED_MAX: AtomicU64 = AtomicU64::new(0);
#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
static ORDERED_LAST: AtomicU64 = AtomicU64::new(0);
#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
static GET_NOW_OFFSET_NANOS: AtomicI64 = AtomicI64::new(0);
#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
static GET_NOW_ALIGNED: AtomicU8 = AtomicU8::new(0);
#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
static PROBE_ORDERED_PROVIDER: AtomicU8 = AtomicU8::new(ORDERED_PERFORMANCE_EPOCH);

const PROBE_READS: usize = 4_096;
const PROBE_SAMPLES: usize = 9;
const REQUIRED_WINS: usize = 8;
const WARMUP_READS: usize = 65_536;

#[cfg(feature = "bench-internal")]
static BENCH_LOCAL_PERFORMANCE_ELIGIBLE: AtomicU8 = AtomicU8::new(0);
#[cfg(feature = "bench-internal")]
static BENCH_LOCAL_HRTIME_ELIGIBLE: AtomicU8 = AtomicU8::new(0);
#[cfg(feature = "bench-internal")]
static BENCH_LOCAL_PERFORMANCE_SAMPLES: [AtomicU64; PROBE_SAMPLES] =
  [const { AtomicU64::new(0) }; PROBE_SAMPLES];
#[cfg(feature = "bench-internal")]
static BENCH_LOCAL_HRTIME_SAMPLES: [AtomicU64; PROBE_SAMPLES] =
  [const { AtomicU64::new(0) }; PROBE_SAMPLES];
#[cfg(feature = "bench-internal")]
static BENCH_LOCAL_ALLOWANCE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internal")]
static BENCH_LOCAL_HRTIME_WINS: AtomicU8 = AtomicU8::new(0);

#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
static BENCH_SHARED_MEMORY: AtomicU8 = AtomicU8::new(0);
#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
static BENCH_PTHREAD_BUILD: AtomicU8 = AtomicU8::new(0);
#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
static BENCH_EPOCH_ELIGIBLE: AtomicU8 = AtomicU8::new(0);
#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
static BENCH_GET_NOW_ELIGIBLE: AtomicU8 = AtomicU8::new(0);
#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
static BENCH_EPOCH_SAMPLES: [AtomicU64; PROBE_SAMPLES] =
  [const { AtomicU64::new(0) }; PROBE_SAMPLES];
#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
static BENCH_GET_NOW_SAMPLES: [AtomicU64; PROBE_SAMPLES] =
  [const { AtomicU64::new(0) }; PROBE_SAMPLES];
#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
static BENCH_ALLOWANCE: AtomicU64 = AtomicU64::new(0);
#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
static BENCH_GET_NOW_WINS: AtomicU8 = AtomicU8::new(0);
#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
static BENCH_GET_NOW_OFFSET_NANOS: AtomicI64 = AtomicI64::new(0);

macro_rules! em_js {
  ($name:ident, $code:literal) => {
    #[used]
    #[unsafe(no_mangle)]
    #[unsafe(link_section = "em_js")]
    #[allow(non_upper_case_globals)]
    static $name: [u8; $code.len()] = *$code;
  };
}

em_js!(
  __em_js__tach_node_thread_cpu_usage_micros,
  b"()<::>{try{const p=globalThis.process;if(p===undefined||typeof p.threadCpuUsage!==\"function\")return -1;const u=p.threadCpuUsage();const n=Number(u.user)+Number(u.system);return Number.isFinite(n)&&n>=0?n:-1}catch(_){return -1}}\0"
);

em_js!(
  __em_js__tach_emscripten_performance_now_millis,
  b"()<::>{let k,s;try{k=Symbol.for(\"tach.emscripten-performance-local.v4\");s=globalThis[k];if(s!==undefined&&s.failed)return -1;if(s===undefined){const p=globalThis.performance;if(p===undefined||p===null||typeof p.now!==\"function\")throw 0;s={read:p.now.bind(p),last:0,failed:false};globalThis[k]=s}const n=s.read();if(!Number.isFinite(n)||n<0||n<s.last)throw 0;s.last=n;return n}catch(_){try{if(s===undefined&&k!==undefined){s={last:0,failed:true};globalThis[k]=s}else if(s!==undefined){s.failed=true}}catch(_){}return -1}}\0"
);

em_js!(
  __em_js__tach_emscripten_performance_last_millis,
  b"()<::>{try{const s=globalThis[Symbol.for(\"tach.emscripten-performance-local.v4\")];return s===undefined?0:s.last}catch(_){return 0}}\0"
);

em_js!(
  __em_js__tach_emscripten_performance_hot_millis,
  b"()<::>{try{const n=tach_emscripten_performance_now_millis();return n>=0?n:tach_emscripten_performance_last_millis()}catch(_){return 0}}\0"
);

em_js!(
  __em_js__tach_emscripten_performance_failed,
  b"()<::>{try{const s=globalThis[Symbol.for(\"tach.emscripten-performance-local.v4\")];return s!==undefined&&s.failed?1:0}catch(_){return 1}}\0"
);

em_js!(
  __em_js__tach_emscripten_node_hrtime_now_millis,
  b"()<::>{let k,s;try{k=Symbol.for(\"tach.node-hrtime-local.v4\");s=globalThis[k];if(s!==undefined&&s.failed)return -1;if(s===undefined){const p=globalThis.process;const h=p===undefined||p===null?null:p.hrtime;if(typeof h!==\"function\"||typeof h.bigint!==\"function\")throw 0;const b=h.bigint.bind(h);const z=b();let o=Number(z)/1e6;try{const q=globalThis.performance;if(q!==undefined&&q!==null&&typeof q.now===\"function\"){const n=q.now.call(q);if(Number.isFinite(n)&&n>=0)o=n}}catch(_){}s={b,z,o,last:o,failed:false};globalThis[k]=s}const n=s.o+Number(s.b()-s.z)/1e6;if(!Number.isFinite(n)||n<0||n<s.last)throw 0;s.last=n;return n}catch(_){try{if(s===undefined&&k!==undefined){s={last:0,failed:true};globalThis[k]=s}else if(s!==undefined){s.failed=true}}catch(_){}return -1}}\0"
);

em_js!(
  __em_js__tach_emscripten_node_hrtime_last_millis,
  b"()<::>{try{const s=globalThis[Symbol.for(\"tach.node-hrtime-local.v4\")];return s===undefined?0:s.last}catch(_){return 0}}\0"
);

em_js!(
  __em_js__tach_emscripten_node_hrtime_hot_millis,
  b"()<::>{try{const n=tach_emscripten_node_hrtime_now_millis();return n>=0?n:tach_emscripten_node_hrtime_last_millis()}catch(_){return 0}}\0"
);

em_js!(
  __em_js__tach_emscripten_node_hrtime_failed,
  b"()<::>{try{const s=globalThis[Symbol.for(\"tach.node-hrtime-local.v4\")];return s!==undefined&&s.failed?1:0}catch(_){return 1}}\0"
);

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
em_js!(
  __em_js__tach_emscripten_performance_epoch_now,
  b"()<::>{let k,s;try{k=Symbol.for(\"tach.performance-origin.v4\");s=globalThis[k];if(s!==undefined&&s.failed)return -1;if(s===undefined){const p=globalThis.performance;if(p===undefined||p===null||typeof p.now!==\"function\"||!Number.isFinite(p.timeOrigin))throw 0;s={origin:p.timeOrigin,read:p.now.bind(p),last:0,failed:false};globalThis[k]=s}const n=s.read();const t=s.origin+n;if(!Number.isFinite(n)||n<0||!Number.isFinite(t)||t<0||t<s.last)throw 0;s.last=t;return t}catch(_){try{if(s===undefined&&k!==undefined){s={last:0,failed:true};globalThis[k]=s}else if(s!==undefined){s.failed=true}}catch(_){}return -1}}\0"
);

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
em_js!(
  __em_js__tach_emscripten_get_now_millis,
  b"()<::>{let k,s;try{k=Symbol.for(\"tach.emscripten-get-now.v4\");s=globalThis[k];if(s!==undefined&&s.failed)return -1;if(s===undefined){s={last:0,failed:false};globalThis[k]=s}const n=_emscripten_get_now();if(!Number.isFinite(n)||n<0||n<s.last)throw 0;s.last=n;return n}catch(_){try{if(s===undefined&&k!==undefined){s={last:0,failed:true};globalThis[k]=s}else if(s!==undefined){s.failed=true}}catch(_){}return -1}}\0"
);

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
em_js!(
  __em_js__tach_emscripten_shared_memory,
  b"()<::>{try{const S=globalThis.SharedArrayBuffer;return typeof S===\"function\"&&HEAP8.buffer instanceof S?1:0}catch(_){return 0}}\0"
);

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
em_js!(
  __em_js__tach_emscripten_pthread_build,
  b"()<::>{try{return typeof PThread===\"object\"&&PThread!==null?1:0}catch(_){return 0}}\0"
);

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
unsafe extern "C" {
  fn emscripten_get_now() -> f64;
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[used]
static LINK_EMSCRIPTEN_GET_NOW: unsafe extern "C" fn() -> f64 = emscripten_get_now;

#[link(wasm_import_module = "env")]
unsafe extern "C" {
  #[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
  fn tach_emscripten_performance_now_millis() -> f64;
  fn tach_emscripten_performance_last_millis() -> f64;
  fn tach_emscripten_performance_hot_millis() -> f64;
  fn tach_emscripten_performance_failed() -> i32;
  fn tach_emscripten_node_hrtime_now_millis() -> f64;
  fn tach_emscripten_node_hrtime_last_millis() -> f64;
  fn tach_emscripten_node_hrtime_hot_millis() -> f64;
  fn tach_emscripten_node_hrtime_failed() -> i32;
  #[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
  fn tach_emscripten_performance_epoch_now() -> f64;
  #[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
  fn tach_emscripten_get_now_millis() -> f64;
  #[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
  fn tach_emscripten_shared_memory() -> i32;
  #[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
  fn tach_emscripten_pthread_build() -> i32;
  fn tach_node_thread_cpu_usage_micros() -> f64;
}

#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
pub(crate) struct BenchOrderedSelectionEvidence {
  pub(crate) selected_provider: &'static str,
  pub(crate) shared_memory: bool,
  pub(crate) pthread_build: bool,
  pub(crate) epoch_eligible: bool,
  pub(crate) get_now_eligible: bool,
  pub(crate) get_now_offset_ns: i64,
  pub(crate) epoch_samples_ns: [u64; PROBE_SAMPLES],
  pub(crate) get_now_samples_ns: [u64; PROBE_SAMPLES],
  pub(crate) allowance_ns: u64,
  pub(crate) get_now_decisive_wins: usize,
}

#[cfg(feature = "bench-internal")]
pub(crate) struct BenchLocalSelectionEvidence {
  pub(crate) selected_provider: &'static str,
  pub(crate) performance_eligible: bool,
  pub(crate) hrtime_eligible: bool,
  pub(crate) performance_samples_ns: [u64; PROBE_SAMPLES],
  pub(crate) hrtime_samples_ns: [u64; PROBE_SAMPLES],
  pub(crate) allowance_ns: u64,
  pub(crate) hrtime_decisive_wins: usize,
}

struct LocalSelectionOutcome {
  provider: u8,
  #[cfg(feature = "bench-internal")]
  performance_eligible: bool,
  #[cfg(feature = "bench-internal")]
  hrtime_eligible: bool,
  #[cfg(feature = "bench-internal")]
  performance_samples: [u64; PROBE_SAMPLES],
  #[cfg(feature = "bench-internal")]
  hrtime_samples: [u64; PROBE_SAMPLES],
  #[cfg(feature = "bench-internal")]
  allowance: u64,
  #[cfg(feature = "bench-internal")]
  hrtime_wins: usize,
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
struct OrderedSelectionOutcome {
  provider: u8,
  #[cfg(feature = "bench-internal")]
  epoch_eligible: bool,
  #[cfg(feature = "bench-internal")]
  get_now_eligible: bool,
  #[cfg(feature = "bench-internal")]
  epoch_samples: [u64; PROBE_SAMPLES],
  #[cfg(feature = "bench-internal")]
  get_now_samples: [u64; PROBE_SAMPLES],
  #[cfg(feature = "bench-internal")]
  allowance: u64,
  #[cfg(feature = "bench-internal")]
  get_now_wins: usize,
}

#[cfg(not(any(feature = "emscripten-pthreads", target_feature = "atomics")))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  // SAFETY: this Emscripten build has no shared Wasm memory or Rust threads.
  // The selector publishes this byte only after its provider is sticky.
  match unsafe { LOCAL_HOT_PROVIDER } {
    LOCAL_PERFORMANCE_NOW => performance_local_ticks(),
    LOCAL_NODE_HRTIME => hrtime_local_ticks(),
    _ => select_local_ticks(),
  }
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks() -> u64 {
  let provider = selected_local_provider();
  read_local_hot(provider).unwrap_or(0)
}

#[cfg(not(any(feature = "emscripten-pthreads", target_feature = "atomics")))]
fn select_local_ticks() -> u64 {
  let provider = selected_local_provider();
  let ticks = match provider {
    LOCAL_PERFORMANCE_NOW => performance_local_ticks(),
    LOCAL_NODE_HRTIME => hrtime_local_ticks(),
    _ => return read_local_hot(provider).unwrap_or(0),
  };
  // SAFETY: non-pthread Emscripten has one Rust execution thread. This byte
  // changes only after the selector has published its sticky provider.
  unsafe { LOCAL_HOT_PROVIDER = provider };
  ticks
}

#[cfg(not(any(feature = "emscripten-pthreads", target_feature = "atomics")))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn performance_local_ticks() -> u64 {
  read_local_hot(LOCAL_PERFORMANCE_NOW).unwrap_or(0)
}

#[cfg(not(any(feature = "emscripten-pthreads", target_feature = "atomics")))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn hrtime_local_ticks() -> u64 {
  read_local_hot(LOCAL_NODE_HRTIME).unwrap_or(0)
}

#[cfg(not(any(feature = "emscripten-pthreads", target_feature = "atomics")))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  ticks()
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered() -> u64 {
  match selected_ordered_provider() {
    provider @ (ORDERED_PERFORMANCE_EPOCH | ORDERED_EMSCRIPTEN_GET_NOW) => {
      read_selected_ordered_clamped(provider)
    }
    ORDERED_LOCAL => ticks(),
    ORDERED_SELECTING => selecting_fallback_ticks(),
    _ => ORDERED_MAX.load(Ordering::SeqCst),
  }
}

#[cfg(not(any(feature = "emscripten-pthreads", target_feature = "atomics")))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unclamped() -> u64 {
  ticks()
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn ticks_ordered_unclamped() -> u64 {
  match selected_ordered_provider() {
    provider @ (ORDERED_PERFORMANCE_EPOCH | ORDERED_EMSCRIPTEN_GET_NOW) => {
      read_selected_ordered_unclamped(provider)
    }
    ORDERED_LOCAL => ticks(),
    ORDERED_SELECTING => {
      selecting_fallback_value().unwrap_or_else(|| ORDERED_LAST.load(Ordering::Relaxed))
    }
    _ => ORDERED_LAST.load(Ordering::Relaxed),
  }
}

#[inline]
pub(crate) fn wall_provider() -> ThreadCpuProvider {
  let provider = selected_local_provider();
  let _ = read_local_hot(provider);
  if local_provider_failed(provider) {
    return ThreadCpuProvider::Unavailable;
  }
  match provider {
    LOCAL_PERFORMANCE_NOW => ThreadCpuProvider::PerformanceNow,
    LOCAL_NODE_HRTIME => ThreadCpuProvider::NodeHrtime,
    _ => ThreadCpuProvider::Unavailable,
  }
}

#[inline]
pub(crate) fn wall_read_cost() -> ThreadCpuReadCost {
  match wall_provider() {
    ThreadCpuProvider::Unavailable => ThreadCpuReadCost::Unavailable,
    _ => ThreadCpuReadCost::HostCall,
  }
}

#[cfg(feature = "bench-internal")]
pub(crate) fn bench_local_selection_evidence() -> BenchLocalSelectionEvidence {
  let selected = selected_local_provider();
  let _ = read_local_hot(selected);
  BenchLocalSelectionEvidence {
    selected_provider: if local_provider_failed(selected) {
      "unavailable"
    } else {
      local_provider_name(selected)
    },
    performance_eligible: BENCH_LOCAL_PERFORMANCE_ELIGIBLE.load(Ordering::Relaxed) != 0,
    hrtime_eligible: BENCH_LOCAL_HRTIME_ELIGIBLE.load(Ordering::Relaxed) != 0,
    performance_samples_ns: core::array::from_fn(|index| {
      BENCH_LOCAL_PERFORMANCE_SAMPLES[index].load(Ordering::Relaxed)
    }),
    hrtime_samples_ns: core::array::from_fn(|index| {
      BENCH_LOCAL_HRTIME_SAMPLES[index].load(Ordering::Relaxed)
    }),
    allowance_ns: BENCH_LOCAL_ALLOWANCE.load(Ordering::Relaxed),
    hrtime_decisive_wins: usize::from(BENCH_LOCAL_HRTIME_WINS.load(Ordering::Relaxed)),
  }
}

#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
pub(crate) fn bench_ordered_selection_evidence() -> BenchOrderedSelectionEvidence {
  let selected = selected_ordered_provider();
  let selected_provider = if selected == ORDERED_LOCAL {
    let local = selected_local_provider();
    let _ = read_local_hot(local);
    if local_provider_failed(local) { "unavailable" } else { local_provider_name(local) }
  } else {
    ordered_provider_name(selected)
  };
  BenchOrderedSelectionEvidence {
    selected_provider,
    shared_memory: BENCH_SHARED_MEMORY.load(Ordering::Relaxed) != 0,
    pthread_build: BENCH_PTHREAD_BUILD.load(Ordering::Relaxed) != 0,
    epoch_eligible: BENCH_EPOCH_ELIGIBLE.load(Ordering::Relaxed) != 0,
    get_now_eligible: BENCH_GET_NOW_ELIGIBLE.load(Ordering::Relaxed) != 0,
    get_now_offset_ns: BENCH_GET_NOW_OFFSET_NANOS.load(Ordering::Relaxed),
    epoch_samples_ns: core::array::from_fn(|index| {
      BENCH_EPOCH_SAMPLES[index].load(Ordering::Relaxed)
    }),
    get_now_samples_ns: core::array::from_fn(|index| {
      BENCH_GET_NOW_SAMPLES[index].load(Ordering::Relaxed)
    }),
    allowance_ns: BENCH_ALLOWANCE.load(Ordering::Relaxed),
    get_now_decisive_wins: usize::from(BENCH_GET_NOW_WINS.load(Ordering::Relaxed)),
  }
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_exact_performance_ticks() -> u64 {
  read_local_hot(LOCAL_PERFORMANCE_NOW).unwrap_or(0)
}

#[cfg(feature = "bench-internal")]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_exact_hrtime_ticks() -> u64 {
  read_local_hot(LOCAL_NODE_HRTIME).unwrap_or(0)
}

#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_exact_ordered_epoch_ticks() -> u64 {
  read_performance_epoch_clamped()
}

#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
pub(crate) fn bench_exact_ordered_aligned_get_now_ticks() -> u64 {
  read_get_now_clamped()
}

#[inline]
fn selected_local_provider() -> u8 {
  loop {
    let provider = LOCAL_STATE.load(Ordering::Acquire);
    match provider {
      LOCAL_UNKNOWN => {
        if LOCAL_STATE
          .compare_exchange(LOCAL_UNKNOWN, LOCAL_SELECTING, Ordering::AcqRel, Ordering::Acquire)
          .is_ok()
        {
          let outcome = detect_local_provider();
          record_local_bench_outcome(&outcome);
          return publish_local_provider(outcome.provider);
        }
      }
      LOCAL_SELECTING => return force_local_reentrant_provider(),
      LOCAL_SELECTING_PERFORMANCE_NOW
      | LOCAL_SELECTING_NODE_HRTIME
      | LOCAL_SELECTING_UNAVAILABLE => return LOCAL_SELECTING,
      _ => return provider,
    }
  }
}

/// Publishes the tournament outcome unless a host callback has reentered the
/// selector and fixed its current probe source as the local timeline.
#[inline]
fn publish_local_provider(provider: u8) -> u8 {
  loop {
    let (expected, selected) = match LOCAL_STATE.load(Ordering::Acquire) {
      LOCAL_SELECTING => (LOCAL_SELECTING, provider),
      LOCAL_SELECTING_PERFORMANCE_NOW => (LOCAL_SELECTING_PERFORMANCE_NOW, LOCAL_PERFORMANCE_NOW),
      LOCAL_SELECTING_NODE_HRTIME => (LOCAL_SELECTING_NODE_HRTIME, LOCAL_NODE_HRTIME),
      LOCAL_SELECTING_UNAVAILABLE => (LOCAL_SELECTING_UNAVAILABLE, LOCAL_UNAVAILABLE),
      selected => return selected,
    };
    if LOCAL_STATE
      .compare_exchange(expected, selected, Ordering::Release, Ordering::Acquire)
      .is_ok()
    {
      return selected;
    }
  }
}

/// Prevents a synchronous JavaScript callback from waiting on the selector it
/// interrupted. The probe source is already the source whose host call is in
/// progress, so making it sticky preserves the nested sample's numeric domain.
/// The nested caller receives `LOCAL_SELECTING` and reads only that source's
/// cached value; it never invokes the callback again.
#[inline]
fn force_local_reentrant_provider() -> u8 {
  let probing = PROBE_LOCAL_PROVIDER.load(Ordering::Acquire);
  let fallback = match probing {
    LOCAL_PERFORMANCE_NOW => LOCAL_SELECTING_PERFORMANCE_NOW,
    LOCAL_NODE_HRTIME => LOCAL_SELECTING_NODE_HRTIME,
    _ => LOCAL_SELECTING_UNAVAILABLE,
  };
  let _ =
    LOCAL_STATE.compare_exchange(LOCAL_SELECTING, fallback, Ordering::AcqRel, Ordering::Acquire);
  LOCAL_SELECTING
}

fn detect_local_provider() -> LocalSelectionOutcome {
  let mut performance_eligible = monotonic_local_pair(LOCAL_PERFORMANCE_NOW);
  let mut hrtime_eligible = monotonic_local_pair(LOCAL_NODE_HRTIME);
  let mut performance_samples = [0; PROBE_SAMPLES];
  let mut hrtime_samples = [0; PROBE_SAMPLES];

  if performance_eligible && hrtime_eligible {
    if !warm_up_local_provider(LOCAL_PERFORMANCE_NOW) {
      performance_eligible = false;
    }
    if !warm_up_local_provider(LOCAL_NODE_HRTIME) {
      hrtime_eligible = false;
    }
  }

  if performance_eligible && hrtime_eligible {
    for sample in 0..PROBE_SAMPLES {
      let pair = if sample & 1 == 0 {
        let performance = measure_local_provider(LOCAL_PERFORMANCE_NOW);
        let hrtime = measure_local_provider(LOCAL_NODE_HRTIME);
        (performance, hrtime)
      } else {
        let hrtime = measure_local_provider(LOCAL_NODE_HRTIME);
        let performance = measure_local_provider(LOCAL_PERFORMANCE_NOW);
        (performance, hrtime)
      };
      match pair {
        (Ok(performance), Ok(hrtime)) => {
          performance_samples[sample] = performance;
          hrtime_samples[sample] = hrtime;
        }
        (Err(LocalMeasureFailure::Source), Err(_)) => {
          performance_eligible = false;
          hrtime_eligible = false;
          break;
        }
        (Err(LocalMeasureFailure::Source), _) => {
          performance_eligible = false;
          break;
        }
        (_, Err(_)) | (Err(LocalMeasureFailure::Timer), _) => {
          hrtime_eligible = false;
          break;
        }
      }
    }
  }

  if !performance_eligible || !hrtime_eligible {
    let provider = match (performance_eligible, hrtime_eligible) {
      (true, false) => LOCAL_PERFORMANCE_NOW,
      (false, true) => LOCAL_NODE_HRTIME,
      _ => LOCAL_UNAVAILABLE,
    };
    return local_selection_outcome(
      provider,
      performance_eligible,
      hrtime_eligible,
      performance_samples,
      hrtime_samples,
      0,
      0,
    );
  }

  let performance_median = median(performance_samples);
  let hrtime_median = median(hrtime_samples);
  let allowance = (performance_median / 20).max(PROBE_READS as u64);
  let hrtime_wins = hrtime_samples
    .iter()
    .zip(performance_samples)
    .filter(|(hrtime, performance)| hrtime.saturating_add(allowance) < *performance)
    .count();
  let hrtime_selected =
    hrtime_wins >= REQUIRED_WINS && hrtime_median.saturating_add(allowance) < performance_median;
  local_selection_outcome(
    if hrtime_selected { LOCAL_NODE_HRTIME } else { LOCAL_PERFORMANCE_NOW },
    true,
    true,
    performance_samples,
    hrtime_samples,
    allowance,
    hrtime_wins,
  )
}

#[allow(clippy::too_many_arguments)]
fn local_selection_outcome(
  provider: u8,
  _performance_eligible: bool,
  _hrtime_eligible: bool,
  _performance_samples: [u64; PROBE_SAMPLES],
  _hrtime_samples: [u64; PROBE_SAMPLES],
  _allowance: u64,
  _hrtime_wins: usize,
) -> LocalSelectionOutcome {
  LocalSelectionOutcome {
    provider,
    #[cfg(feature = "bench-internal")]
    performance_eligible: _performance_eligible,
    #[cfg(feature = "bench-internal")]
    hrtime_eligible: _hrtime_eligible,
    #[cfg(feature = "bench-internal")]
    performance_samples: _performance_samples,
    #[cfg(feature = "bench-internal")]
    hrtime_samples: _hrtime_samples,
    #[cfg(feature = "bench-internal")]
    allowance: _allowance,
    #[cfg(feature = "bench-internal")]
    hrtime_wins: _hrtime_wins,
  }
}

#[derive(Clone, Copy)]
enum LocalMeasureFailure {
  Source,
  Timer,
}

fn warm_up_local_provider(provider: u8) -> bool {
  PROBE_LOCAL_PROVIDER.store(provider, Ordering::Release);
  let mut previous = 0;
  let mut sink = 0;
  for _ in 0..WARMUP_READS {
    let Some(ticks) = read_probe_local() else {
      return false;
    };
    if ticks < previous {
      return false;
    }
    previous = ticks;
    sink ^= ticks;
  }
  core::hint::black_box(sink);
  !local_provider_failed(provider)
}

fn measure_local_provider(provider: u8) -> Result<u64, LocalMeasureFailure> {
  PROBE_LOCAL_PROVIDER.store(provider, Ordering::Release);
  let start = read_local_measurement_timer().ok_or(LocalMeasureFailure::Timer)?;
  let mut previous = 0;
  let mut sink = 0;
  for _ in 0..PROBE_READS {
    let ticks = read_probe_local().ok_or(LocalMeasureFailure::Source)?;
    if ticks < previous {
      return Err(LocalMeasureFailure::Source);
    }
    previous = ticks;
    sink ^= ticks;
  }
  core::hint::black_box(sink);
  if local_provider_failed(provider) {
    return Err(LocalMeasureFailure::Source);
  }
  let end = read_local_measurement_timer().ok_or(LocalMeasureFailure::Timer)?;
  end.checked_sub(start).ok_or(LocalMeasureFailure::Timer)
}

fn monotonic_local_pair(provider: u8) -> bool {
  PROBE_LOCAL_PROVIDER.store(provider, Ordering::Release);
  let Some(first) = read_local_hot(provider) else {
    return false;
  };
  read_local_hot(provider).is_some_and(|second| second >= first) && !local_provider_failed(provider)
}

#[inline(always)]
#[allow(clippy::inline_always)]
fn read_probe_local() -> Option<u64> {
  read_local_hot(PROBE_LOCAL_PROVIDER.load(Ordering::Acquire))
}

#[inline]
fn read_local_measurement_timer() -> Option<u64> {
  let previous = PROBE_LOCAL_PROVIDER.swap(LOCAL_NODE_HRTIME, Ordering::AcqRel);
  let ticks = node_hrtime_nanos();
  PROBE_LOCAL_PROVIDER.store(previous, Ordering::Release);
  ticks
}

#[inline]
fn read_local_hot(provider: u8) -> Option<u64> {
  let millis = match provider {
    LOCAL_SELECTING => return Some(read_local_selecting_fallback()),
    LOCAL_PERFORMANCE_NOW => {
      // SAFETY: the linked shim catches host failures and freezes the provider.
      unsafe { tach_emscripten_performance_hot_millis() }
    }
    LOCAL_NODE_HRTIME => {
      // SAFETY: the linked shim catches host failures and freezes the provider.
      unsafe { tach_emscripten_node_hrtime_hot_millis() }
    }
    _ => return None,
  };
  millis_to_nanos(millis)
}

#[inline]
fn read_local_selecting_fallback() -> u64 {
  let state = LOCAL_STATE.load(Ordering::Acquire);
  let provider = match state {
    LOCAL_SELECTING_PERFORMANCE_NOW | LOCAL_PERFORMANCE_NOW => LOCAL_PERFORMANCE_NOW,
    LOCAL_SELECTING_NODE_HRTIME | LOCAL_NODE_HRTIME => LOCAL_NODE_HRTIME,
    _ => PROBE_LOCAL_PROVIDER.load(Ordering::Acquire),
  };
  let millis = match provider {
    LOCAL_PERFORMANCE_NOW => {
      // SAFETY: the linked shim reads the cached value without calling the host clock.
      unsafe { tach_emscripten_performance_last_millis() }
    }
    LOCAL_NODE_HRTIME => {
      // SAFETY: the linked shim reads the cached value without calling the host clock.
      unsafe { tach_emscripten_node_hrtime_last_millis() }
    }
    _ => 0.0,
  };
  millis_to_nanos(millis).unwrap_or(0)
}

#[inline]
fn local_provider_failed(provider: u8) -> bool {
  match provider {
    LOCAL_PERFORMANCE_NOW => {
      // SAFETY: the linked shim only reads the current realm's provider state.
      unsafe { tach_emscripten_performance_failed() != 0 }
    }
    LOCAL_NODE_HRTIME => {
      // SAFETY: the linked shim only reads the current realm's provider state.
      unsafe { tach_emscripten_node_hrtime_failed() != 0 }
    }
    _ => true,
  }
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn performance_now_nanos() -> Option<u64> {
  // SAFETY: the linked shim catches host exceptions and returns `-1`.
  millis_to_nanos(unsafe { tach_emscripten_performance_now_millis() })
}

#[inline]
fn node_hrtime_nanos() -> Option<u64> {
  // SAFETY: the linked shim catches missing or throwing Node globals and
  // returns `-1`.
  millis_to_nanos(unsafe { tach_emscripten_node_hrtime_now_millis() })
}

#[cfg(feature = "bench-internal")]
fn record_local_bench_outcome(outcome: &LocalSelectionOutcome) {
  BENCH_LOCAL_PERFORMANCE_ELIGIBLE.store(u8::from(outcome.performance_eligible), Ordering::Relaxed);
  BENCH_LOCAL_HRTIME_ELIGIBLE.store(u8::from(outcome.hrtime_eligible), Ordering::Relaxed);
  for index in 0..PROBE_SAMPLES {
    BENCH_LOCAL_PERFORMANCE_SAMPLES[index]
      .store(outcome.performance_samples[index], Ordering::Relaxed);
    BENCH_LOCAL_HRTIME_SAMPLES[index].store(outcome.hrtime_samples[index], Ordering::Relaxed);
  }
  BENCH_LOCAL_ALLOWANCE.store(outcome.allowance, Ordering::Relaxed);
  BENCH_LOCAL_HRTIME_WINS.store(outcome.hrtime_wins as u8, Ordering::Relaxed);
}

#[cfg(not(feature = "bench-internal"))]
fn record_local_bench_outcome(_: &LocalSelectionOutcome) {}

#[cfg(feature = "bench-internal")]
const fn local_provider_name(provider: u8) -> &'static str {
  match provider {
    LOCAL_PERFORMANCE_NOW => "performance.now",
    LOCAL_NODE_HRTIME => "process.hrtime.bigint",
    _ => "unavailable",
  }
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn selected_ordered_provider() -> u8 {
  let provider = ORDERED_STATE.load(Ordering::Acquire);
  if provider != 0 {
    return provider;
  }
  if ORDERED_STATE
    .compare_exchange(0, ORDERED_SELECTING, Ordering::AcqRel, Ordering::Acquire)
    .is_err()
  {
    return ORDERED_STATE.load(Ordering::Acquire);
  }

  let shared = shared_memory_enabled();
  let pthread = pthread_build_enabled();
  let outcome = detect_ordered_provider(shared, pthread);
  record_bench_outcome(shared, pthread, &outcome);
  ORDERED_STATE.store(outcome.provider, Ordering::Release);
  outcome.provider
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
fn detect_ordered_provider(shared: bool, pthread: bool) -> OrderedSelectionOutcome {
  if !shared {
    GET_NOW_ALIGNED.store(0, Ordering::Relaxed);
    return unmeasured_outcome(
      provider_without_tournament(false, false, false, false).unwrap_or(ORDERED_LOCAL),
      false,
      false,
    );
  }

  let mut epoch_eligible = monotonic_pair(performance_epoch_nanos);
  let mut get_now_eligible = pthread && epoch_eligible && monotonic_pair(raw_get_now_nanos);
  let get_now_aligned = epoch_eligible
    && get_now_eligible
    && align_get_now_to_epoch().is_some()
    && monotonic_pair(aligned_get_now_nanos_ready)
    && aligned_get_now_matches_epoch();
  if !get_now_aligned {
    GET_NOW_ALIGNED.store(0, Ordering::Relaxed);
  }
  if let Some(provider) =
    provider_without_tournament(true, epoch_eligible, get_now_eligible, get_now_aligned)
  {
    return unmeasured_outcome(provider, epoch_eligible, get_now_eligible && get_now_aligned);
  }

  let mut epoch_samples = [0; PROBE_SAMPLES];
  let mut get_now_samples = [0; PROBE_SAMPLES];
  if !warm_up_provider(ORDERED_PERFORMANCE_EPOCH) {
    epoch_eligible = false;
  }
  if !warm_up_provider(ORDERED_EMSCRIPTEN_GET_NOW) {
    get_now_eligible = false;
  }

  if epoch_eligible && get_now_eligible {
    for sample in 0..PROBE_SAMPLES {
      let pair = if sample & 1 == 0 {
        let epoch = measure_provider(ORDERED_PERFORMANCE_EPOCH);
        let get_now = measure_provider(ORDERED_EMSCRIPTEN_GET_NOW);
        (epoch, get_now)
      } else {
        let get_now = measure_provider(ORDERED_EMSCRIPTEN_GET_NOW);
        let epoch = measure_provider(ORDERED_PERFORMANCE_EPOCH);
        (epoch, get_now)
      };
      match pair {
        (Ok(epoch), Ok(get_now)) => {
          epoch_samples[sample] = epoch;
          get_now_samples[sample] = get_now;
        }
        (Err(OrderedMeasureFailure::Source), Err(OrderedMeasureFailure::Source)) => {
          epoch_eligible = false;
          get_now_eligible = false;
          break;
        }
        (Err(OrderedMeasureFailure::Source), _) => {
          epoch_eligible = false;
          break;
        }
        (_, Err(OrderedMeasureFailure::Source)) => {
          get_now_eligible = false;
          break;
        }
        (Err(OrderedMeasureFailure::Timer), _) | (_, Err(OrderedMeasureFailure::Timer)) => {
          return ordered_selection_outcome(
            ORDERED_PERFORMANCE_EPOCH,
            true,
            true,
            epoch_samples,
            get_now_samples,
            0,
            0,
          );
        }
      }
    }
  }

  if !epoch_eligible || !get_now_eligible {
    let provider = match (epoch_eligible, get_now_eligible) {
      (true, false) => ORDERED_PERFORMANCE_EPOCH,
      (false, true) => ORDERED_EMSCRIPTEN_GET_NOW,
      _ => ORDERED_UNAVAILABLE,
    };
    return ordered_selection_outcome(
      provider,
      epoch_eligible,
      get_now_eligible,
      epoch_samples,
      get_now_samples,
      0,
      0,
    );
  }

  let epoch_median = median(epoch_samples);
  let get_now_median = median(get_now_samples);
  let allowance = (epoch_median / 20).max(PROBE_READS as u64);
  let get_now_wins = get_now_samples
    .iter()
    .zip(epoch_samples)
    .filter(|(get_now, epoch)| get_now.saturating_add(allowance) < *epoch)
    .count();
  let get_now_selected =
    get_now_wins >= REQUIRED_WINS && get_now_median.saturating_add(allowance) < epoch_median;
  ordered_selection_outcome(
    if get_now_selected { ORDERED_EMSCRIPTEN_GET_NOW } else { ORDERED_PERFORMANCE_EPOCH },
    epoch_eligible,
    get_now_eligible,
    epoch_samples,
    get_now_samples,
    allowance,
    get_now_wins,
  )
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[allow(clippy::too_many_arguments)]
fn ordered_selection_outcome(
  provider: u8,
  _epoch_eligible: bool,
  _get_now_eligible: bool,
  _epoch_samples: [u64; PROBE_SAMPLES],
  _get_now_samples: [u64; PROBE_SAMPLES],
  _allowance: u64,
  _get_now_wins: usize,
) -> OrderedSelectionOutcome {
  OrderedSelectionOutcome {
    provider,
    #[cfg(feature = "bench-internal")]
    epoch_eligible: _epoch_eligible,
    #[cfg(feature = "bench-internal")]
    get_now_eligible: _get_now_eligible,
    #[cfg(feature = "bench-internal")]
    epoch_samples: _epoch_samples,
    #[cfg(feature = "bench-internal")]
    get_now_samples: _get_now_samples,
    #[cfg(feature = "bench-internal")]
    allowance: _allowance,
    #[cfg(feature = "bench-internal")]
    get_now_wins: _get_now_wins,
  }
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
const fn provider_without_tournament(
  shared: bool,
  epoch_eligible: bool,
  get_now_eligible: bool,
  get_now_aligned: bool,
) -> Option<u8> {
  if !shared {
    return Some(ORDERED_LOCAL);
  }
  match (epoch_eligible, get_now_eligible, get_now_aligned) {
    (false, false, _) => Some(ORDERED_UNAVAILABLE),
    (true, false, _) => Some(ORDERED_PERFORMANCE_EPOCH),
    (false, true, false) => Some(ORDERED_UNAVAILABLE),
    (false, true, true) => Some(ORDERED_EMSCRIPTEN_GET_NOW),
    // A failed alignment means the two clocks cannot safely share the atomic
    // maximum. Keep the epoch incumbent instead of mixing raw domains.
    (true, true, false) => Some(ORDERED_PERFORMANCE_EPOCH),
    (true, true, true) => None,
  }
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
fn unmeasured_outcome(
  provider: u8,
  _epoch_eligible: bool,
  _get_now_eligible: bool,
) -> OrderedSelectionOutcome {
  OrderedSelectionOutcome {
    provider,
    #[cfg(feature = "bench-internal")]
    epoch_eligible: _epoch_eligible,
    #[cfg(feature = "bench-internal")]
    get_now_eligible: _get_now_eligible,
    #[cfg(feature = "bench-internal")]
    epoch_samples: [0; PROBE_SAMPLES],
    #[cfg(feature = "bench-internal")]
    get_now_samples: [0; PROBE_SAMPLES],
    #[cfg(feature = "bench-internal")]
    allowance: 0,
    #[cfg(feature = "bench-internal")]
    get_now_wins: 0,
  }
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn shared_memory_enabled() -> bool {
  // SAFETY: the generated shim only inspects Emscripten's current linear-memory
  // buffer and returns false if that runtime state is unavailable.
  unsafe { tach_emscripten_shared_memory() != 0 }
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn pthread_build_enabled() -> bool {
  // SAFETY: the generated shim only tests for Emscripten's pthread runtime
  // object and reports false when it is absent.
  unsafe { tach_emscripten_pthread_build() != 0 }
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn performance_epoch_nanos() -> Option<u64> {
  // SAFETY: the generated shim catches host failures and reports them as -1.
  millis_to_nanos(unsafe { tach_emscripten_performance_epoch_now() })
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn raw_get_now_nanos() -> Option<u64> {
  // SAFETY: the linked shim catches host exceptions and returns `-1`.
  millis_to_nanos(unsafe { tach_emscripten_get_now_millis() })
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
fn align_get_now_to_epoch() -> Option<i64> {
  let before = performance_epoch_nanos()?;
  let raw = raw_get_now_nanos()?;
  let after = performance_epoch_nanos()?;
  let midpoint = before.checked_add(after.checked_sub(before)? / 2)?;
  let offset = i128::from(midpoint) - i128::from(raw);
  let offset = i64::try_from(offset).ok()?;
  GET_NOW_OFFSET_NANOS.store(offset, Ordering::Relaxed);
  GET_NOW_ALIGNED.store(1, Ordering::Relaxed);
  Some(offset)
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn aligned_get_now_nanos_ready() -> Option<u64> {
  apply_signed_offset(raw_get_now_nanos()?, GET_NOW_OFFSET_NANOS.load(Ordering::Relaxed))
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn selected_get_now_nanos() -> Option<u64> {
  if GET_NOW_ALIGNED.load(Ordering::Relaxed) == 0 {
    raw_get_now_nanos()
  } else {
    aligned_get_now_nanos_ready()
  }
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
fn aligned_get_now_matches_epoch() -> bool {
  let Some(before) = performance_epoch_nanos() else {
    return false;
  };
  let Some(aligned) = aligned_get_now_nanos_ready() else {
    return false;
  };
  let Some(after) = performance_epoch_nanos() else {
    return false;
  };
  aligned >= before.saturating_sub(1_000_000) && aligned <= after.saturating_add(1_000_000)
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
fn apply_signed_offset(value: u64, offset: i64) -> Option<u64> {
  if offset >= 0 {
    value.checked_add(offset as u64)
  } else {
    value.checked_sub(offset.unsigned_abs())
  }
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn read_performance_epoch_value() -> Option<u64> {
  performance_epoch_nanos()
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn read_get_now_value() -> Option<u64> {
  selected_get_now_nanos()
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn read_ordered_provider(provider: u8) -> Option<u64> {
  match provider {
    ORDERED_PERFORMANCE_EPOCH => read_performance_epoch_value(),
    ORDERED_EMSCRIPTEN_GET_NOW => read_get_now_value(),
    _ => None,
  }
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn read_selected_ordered_clamped(provider: u8) -> u64 {
  let Some(ticks) = read_ordered_provider(provider) else {
    fail_ordered_provider(provider);
    return ORDERED_MAX.load(Ordering::SeqCst);
  };
  ORDERED_LAST.fetch_max(ticks, Ordering::Relaxed);
  clamp_ordered(ticks)
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn read_selected_ordered_unclamped(provider: u8) -> u64 {
  let Some(ticks) = read_ordered_provider(provider) else {
    fail_ordered_provider(provider);
    return ORDERED_LAST.load(Ordering::Relaxed);
  };
  ORDERED_LAST.fetch_max(ticks, Ordering::Relaxed);
  ticks
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn fail_ordered_provider(provider: u8) {
  let _ = ORDERED_STATE.compare_exchange(
    provider,
    ORDERED_UNAVAILABLE,
    Ordering::AcqRel,
    Ordering::Acquire,
  );
}

#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn read_performance_epoch_clamped() -> u64 {
  read_performance_epoch_value().map_or_else(
    || ORDERED_MAX.load(Ordering::SeqCst),
    |ticks| {
      ORDERED_LAST.fetch_max(ticks, Ordering::Relaxed);
      clamp_ordered(ticks)
    },
  )
}

#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn read_get_now_clamped() -> u64 {
  read_get_now_value().map_or_else(
    || ORDERED_MAX.load(Ordering::SeqCst),
    |ticks| {
      ORDERED_LAST.fetch_max(ticks, Ordering::Relaxed);
      clamp_ordered(ticks)
    },
  )
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn read_probe_ordered() -> Option<u64> {
  read_ordered_provider(PROBE_ORDERED_PROVIDER.load(Ordering::Acquire)).map(|ticks| {
    ORDERED_LAST.fetch_max(ticks, Ordering::Relaxed);
    clamp_ordered(ticks)
  })
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline(always)]
#[allow(clippy::inline_always)]
fn clamp_ordered(ticks: u64) -> u64 {
  ticks.max(ORDERED_MAX.fetch_max(ticks, Ordering::SeqCst))
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn selecting_fallback_value() -> Option<u64> {
  if !shared_memory_enabled() {
    return Some(ticks());
  }
  performance_epoch_nanos().or_else(|| {
    (pthread_build_enabled() && GET_NOW_ALIGNED.load(Ordering::Relaxed) != 0)
      .then(aligned_get_now_nanos_ready)
      .flatten()
  })
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn selecting_fallback_ticks() -> u64 {
  clamp_ordered(selecting_fallback_value().unwrap_or(0))
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
fn warm_up_provider(provider: u8) -> bool {
  PROBE_ORDERED_PROVIDER.store(provider, Ordering::Release);
  let mut previous = 0;
  let mut sink = 0;
  for _ in 0..WARMUP_READS {
    let Some(ticks) = read_probe_ordered() else {
      return false;
    };
    if ticks < previous {
      return false;
    }
    previous = ticks;
    sink ^= ticks;
  }
  core::hint::black_box(sink);
  true
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[derive(Clone, Copy)]
enum OrderedMeasureFailure {
  Source,
  Timer,
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
fn measure_provider(provider: u8) -> Result<u64, OrderedMeasureFailure> {
  PROBE_ORDERED_PROVIDER.store(provider, Ordering::Release);
  let start = performance_timer_nanos().ok_or(OrderedMeasureFailure::Timer)?;
  let mut previous = 0;
  let mut sink = 0;
  for _ in 0..PROBE_READS {
    let ticks = read_probe_ordered().ok_or(OrderedMeasureFailure::Source)?;
    if ticks < previous {
      return Err(OrderedMeasureFailure::Source);
    }
    previous = ticks;
    sink ^= ticks;
  }
  core::hint::black_box(sink);
  let end = performance_timer_nanos().ok_or(OrderedMeasureFailure::Timer)?;
  end.checked_sub(start).ok_or(OrderedMeasureFailure::Timer)
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
#[inline]
fn performance_timer_nanos() -> Option<u64> {
  node_hrtime_nanos().or_else(performance_now_nanos)
}

#[cfg(any(feature = "emscripten-pthreads", target_feature = "atomics"))]
fn monotonic_pair(read: fn() -> Option<u64>) -> bool {
  let Some(first) = read() else {
    return false;
  };
  read().is_some_and(|second| second >= first)
}

fn median(mut samples: [u64; PROBE_SAMPLES]) -> u64 {
  samples.sort_unstable();
  samples[PROBE_SAMPLES / 2]
}

#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
fn record_bench_outcome(shared: bool, pthread: bool, outcome: &OrderedSelectionOutcome) {
  BENCH_SHARED_MEMORY.store(u8::from(shared), Ordering::Relaxed);
  BENCH_PTHREAD_BUILD.store(u8::from(pthread), Ordering::Relaxed);
  BENCH_EPOCH_ELIGIBLE.store(u8::from(outcome.epoch_eligible), Ordering::Relaxed);
  BENCH_GET_NOW_ELIGIBLE.store(u8::from(outcome.get_now_eligible), Ordering::Relaxed);
  BENCH_GET_NOW_OFFSET_NANOS.store(GET_NOW_OFFSET_NANOS.load(Ordering::Relaxed), Ordering::Relaxed);
  for index in 0..PROBE_SAMPLES {
    BENCH_EPOCH_SAMPLES[index].store(outcome.epoch_samples[index], Ordering::Relaxed);
    BENCH_GET_NOW_SAMPLES[index].store(outcome.get_now_samples[index], Ordering::Relaxed);
  }
  BENCH_ALLOWANCE.store(outcome.allowance, Ordering::Relaxed);
  BENCH_GET_NOW_WINS.store(outcome.get_now_wins as u8, Ordering::Relaxed);
}

#[cfg(all(
  not(feature = "bench-internal"),
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
fn record_bench_outcome(_: bool, _: bool, _: &OrderedSelectionOutcome) {}

#[cfg(all(
  feature = "bench-internal",
  any(feature = "emscripten-pthreads", target_feature = "atomics"),
))]
const fn ordered_provider_name(provider: u8) -> &'static str {
  match provider {
    ORDERED_PERFORMANCE_EPOCH => "emscripten_performance_epoch_atomic_max",
    ORDERED_EMSCRIPTEN_GET_NOW => "emscripten_get_now_atomic_max",
    _ => "unavailable",
  }
}

#[inline]
fn millis_to_nanos(millis: f64) -> Option<u64> {
  if !millis.is_finite() || millis < 0.0 {
    return None;
  }
  let nanos = millis * 1_000_000.0;
  if nanos >= u64::MAX as f64 {
    return None;
  }
  Some(nanos as u64)
}

#[inline]
pub(crate) fn node_thread_cpu_usage_micros() -> Option<u64> {
  // SAFETY: the linked shim catches host failures and reports -1.
  let micros = unsafe { tach_node_thread_cpu_usage_micros() };
  if !micros.is_finite() || micros < 0.0 {
    return None;
  }
  Some(micros as u64)
}

#[cfg(all(test, any(feature = "emscripten-pthreads", target_feature = "atomics"),))]
mod tests {
  use super::*;

  #[test]
  fn median_uses_the_middle_of_nine_samples() {
    assert_eq!(median([9, 1, 8, 2, 7, 3, 6, 4, 5]), 5);
  }

  #[test]
  fn provider_routing_preserves_local_and_standalone_domains() {
    assert_eq!(provider_without_tournament(false, false, false, false), Some(ORDERED_LOCAL));
    assert_eq!(
      provider_without_tournament(true, true, false, false),
      Some(ORDERED_PERFORMANCE_EPOCH)
    );
    assert_eq!(provider_without_tournament(true, false, true, false), Some(ORDERED_UNAVAILABLE));
    assert_eq!(
      provider_without_tournament(true, false, true, true),
      Some(ORDERED_EMSCRIPTEN_GET_NOW)
    );
    assert_eq!(
      provider_without_tournament(true, true, true, false),
      Some(ORDERED_PERFORMANCE_EPOCH)
    );
    assert_eq!(provider_without_tournament(true, true, true, true), None);
  }

  #[test]
  fn material_winner_requires_eight_decisive_batches() {
    let epoch: [u64; PROBE_SAMPLES] = [100_000; PROBE_SAMPLES];
    let mut get_now: [u64; PROBE_SAMPLES] = [80_000; PROBE_SAMPLES];
    get_now[8] = 100_000;
    let allowance = (median(epoch) / 20).max(PROBE_READS as u64);
    let wins = get_now
      .iter()
      .zip(epoch)
      .filter(|(challenger, incumbent)| (**challenger).saturating_add(allowance) < *incumbent)
      .count();
    assert_eq!(wins, REQUIRED_WINS);

    get_now[7] = 100_000;
    let wins = get_now
      .iter()
      .zip(epoch)
      .filter(|(challenger, incumbent)| (**challenger).saturating_add(allowance) < *incumbent)
      .count();
    assert_eq!(wins, REQUIRED_WINS - 1);
  }

  #[test]
  fn signed_alignment_rejects_overflow_and_underflow() {
    assert_eq!(apply_signed_offset(100, 25), Some(125));
    assert_eq!(apply_signed_offset(100, -25), Some(75));
    assert_eq!(apply_signed_offset(u64::MAX, 1), None);
    assert_eq!(apply_signed_offset(0, -1), None);
  }

  #[test]
  fn host_milliseconds_reject_invalid_values() {
    assert_eq!(millis_to_nanos(1.5), Some(1_500_000));
    assert_eq!(millis_to_nanos(-1.0), None);
    assert_eq!(millis_to_nanos(f64::INFINITY), None);
    assert_eq!(millis_to_nanos(f64::NAN), None);
  }
}

#[cfg(all(test, target_os = "emscripten"))]
mod local_selection_tests {
  use super::*;

  #[test]
  fn reentrant_local_selection_forces_the_probed_domain() {
    let previous_state = LOCAL_STATE.swap(LOCAL_SELECTING, Ordering::AcqRel);
    let previous_probe = PROBE_LOCAL_PROVIDER.swap(LOCAL_PERFORMANCE_NOW, Ordering::AcqRel);

    assert_eq!(selected_local_provider(), LOCAL_SELECTING);
    assert_eq!(LOCAL_STATE.load(Ordering::Acquire), LOCAL_SELECTING_PERFORMANCE_NOW);
    assert_eq!(publish_local_provider(LOCAL_NODE_HRTIME), LOCAL_PERFORMANCE_NOW);

    LOCAL_STATE.store(previous_state, Ordering::Release);
    PROBE_LOCAL_PROVIDER.store(previous_probe, Ordering::Release);
  }
}
