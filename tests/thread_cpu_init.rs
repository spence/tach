use std::sync::{Arc, Barrier};

use tach::ThreadCpuInstant;

#[test]
fn concurrent_first_reads_never_fail() {
  const THREADS: usize = 16;
  let barrier = Arc::new(Barrier::new(THREADS));
  let workers: Vec<_> = (0..THREADS)
    .map(|_| {
      let barrier = Arc::clone(&barrier);
      std::thread::spawn(move || {
        barrier.wait();
        let start = ThreadCpuInstant::now();
        let provider = ThreadCpuInstant::provider();
        let end = ThreadCpuInstant::now();
        assert!(end.checked_duration_since(start).is_some());
        provider
      })
    })
    .collect();

  for worker in workers {
    let provider = worker.join().expect("provider initialization panicked");
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    assert!(provider.measures_thread_cpu_time(), "unexpected provider: {provider:?}");
  }
}
