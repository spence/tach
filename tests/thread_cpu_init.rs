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
    let _ = provider;
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    assert!(provider.measures_thread_cpu_time(), "unexpected provider: {provider:?}");
  }
}

#[cfg(target_os = "linux")]
mod linux_signal_reentry {
  use std::cell::Cell;
  use std::sync::Arc;
  use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

  use tach::ThreadCpuInstant;

  static SIGNAL_COUNT: AtomicUsize = AtomicUsize::new(0);
  static SIGNAL_FAILED: AtomicBool = AtomicBool::new(false);

  std::thread_local! {
    static SIGNAL_LAST: Cell<Option<ThreadCpuInstant>> = const { Cell::new(None) };
  }

  unsafe extern "C" fn sample_thread_cpu(_: libc::c_int) {
    let sample = ThreadCpuInstant::now();
    if !sample.measures_thread_cpu_time() {
      SIGNAL_FAILED.store(true, Ordering::Relaxed);
    }
    if SIGNAL_LAST
      .try_with(|last| {
        if let Some(previous) = last.get()
          && sample.checked_duration_since(previous).is_none()
        {
          SIGNAL_FAILED.store(true, Ordering::Relaxed);
        }
        last.set(Some(sample));
      })
      .is_err()
    {
      SIGNAL_FAILED.store(true, Ordering::Relaxed);
    }
    SIGNAL_COUNT.fetch_add(1, Ordering::Relaxed);
  }

  #[test]
  fn signal_reentry_cannot_poison_or_reverse_first_read_selection() {
    SIGNAL_COUNT.store(0, Ordering::Relaxed);
    SIGNAL_FAILED.store(false, Ordering::Relaxed);

    let mut action = unsafe { core::mem::zeroed::<libc::sigaction>() };
    action.sa_sigaction = sample_thread_cpu as *const () as usize;
    // SAFETY: the set is writable and then belongs to this sigaction value.
    unsafe { libc::sigemptyset(&mut action.sa_mask) };
    let mut previous = unsafe { core::mem::zeroed::<libc::sigaction>() };
    // SAFETY: both sigaction pointers are valid for SIGUSR1.
    assert_eq!(unsafe { libc::sigaction(libc::SIGUSR1, &action, &mut previous) }, 0);

    let (target_tx, target_rx) = std::sync::mpsc::sync_channel(1);
    let done = Arc::new(AtomicBool::new(false));
    let worker_done = Arc::clone(&done);
    let worker = std::thread::spawn(move || {
      // SAFETY: pthread_self has no caller-side preconditions.
      target_tx.send(unsafe { libc::pthread_self() } as usize).unwrap();
      let start = ThreadCpuInstant::now();
      while SIGNAL_COUNT.load(Ordering::Relaxed) == 0 {
        core::hint::spin_loop();
      }
      let end = ThreadCpuInstant::now();
      let last_signal = SIGNAL_LAST.with(Cell::get);
      let monotonic = end.checked_duration_since(start).is_some()
        && last_signal
          .is_none_or(|signal_sample| end.checked_duration_since(signal_sample).is_some());
      let provider = ThreadCpuInstant::provider();
      let mut blocked = unsafe { core::mem::zeroed::<libc::sigset_t>() };
      // SAFETY: `blocked` is writable signal-set storage local to this thread.
      unsafe {
        libc::sigemptyset(&mut blocked);
        libc::sigaddset(&mut blocked, libc::SIGUSR1);
      }
      // Stop delivery before publishing completion so an already-issued
      // signal cannot enter the handler during this thread's TLS teardown.
      // SAFETY: the set is initialized above and the old mask is not needed
      // because this worker exits immediately after returning its result.
      let mask_status =
        unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &blocked, core::ptr::null_mut()) };
      worker_done.store(true, Ordering::Release);
      (monotonic, provider, mask_status)
    });

    let target = target_rx.recv().unwrap() as libc::pthread_t;
    while !done.load(Ordering::Acquire) {
      // SAFETY: target names the live worker and SIGUSR1 has the handler above.
      let status = unsafe { libc::pthread_kill(target, libc::SIGUSR1) };
      if status != 0 {
        break;
      }
    }

    let (monotonic, provider, mask_status) = worker.join().unwrap();
    // SAFETY: restore the process's prior SIGUSR1 disposition.
    assert_eq!(unsafe { libc::sigaction(libc::SIGUSR1, &previous, core::ptr::null_mut()) }, 0);

    assert!(SIGNAL_COUNT.load(Ordering::Relaxed) > 0);
    assert_eq!(mask_status, 0);
    assert!(!SIGNAL_FAILED.load(Ordering::Relaxed));
    assert!(provider.measures_thread_cpu_time(), "unexpected provider: {provider:?}");
    assert!(monotonic);
  }
}
