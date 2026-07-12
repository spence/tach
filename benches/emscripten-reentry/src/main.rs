//! Executable regression probe for Emscripten local-clock selection reentry.
//!
//! `../run-emscripten-reentry.sh` injects a JavaScript `performance.now()`
//! wrapper that calls `tach_emscripten_reentry_now` while `Instant::now()` is
//! selecting its first provider. The program exits successfully only when the
//! nested call returns and the selector publishes one compatible local domain.

#[cfg(target_os = "emscripten")]
use core::sync::atomic::{AtomicUsize, Ordering};

#[cfg(target_os = "emscripten")]
static REENTRIES: AtomicUsize = AtomicUsize::new(0);

#[cfg(target_os = "emscripten")]
#[unsafe(no_mangle)]
pub extern "C" fn tach_emscripten_reentry_now() -> u64 {
  REENTRIES.fetch_add(1, Ordering::SeqCst);
  let start = tach::Instant::now();
  u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(target_os = "emscripten")]
fn main() {
  let start = tach::Instant::now();
  let _ = start.elapsed();
  assert_eq!(
    REENTRIES.load(Ordering::SeqCst),
    1,
    "the injected performance.now callback must reenter once",
  );
}

#[cfg(not(target_os = "emscripten"))]
fn main() {
  eprintln!("this probe runs through benches/run-emscripten-reentry.sh");
}
