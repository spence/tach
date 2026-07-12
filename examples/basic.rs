use tach::{Instant, OrderedInstant, ThreadCpuInstant};

fn main() {
  // Prime the lazy frequency calibration so the first measurement
  // doesn't include the one-time ~50ms calibration cost.
  let _ = Instant::now().elapsed();

  let start = Instant::now();
  let mut sum = 0u64;
  for i in 0..1_000_000 {
    sum = std::hint::black_box(sum.wrapping_add(i));
  }
  let elapsed = start.elapsed();

  println!("1M additions (sum = {sum}):");
  println!("  local elapsed = {elapsed:?}");

  let published = OrderedInstant::now();
  let ordered_elapsed = std::thread::spawn(move || published.elapsed())
    .join()
    .expect("worker should not panic");
  println!("  ordered cross-thread elapsed = {ordered_elapsed:?}");

  let cpu_start = ThreadCpuInstant::now();
  let mut product = 1_u64;
  for i in 1..10_000 {
    product = std::hint::black_box(product.wrapping_mul(i));
  }
  let cpu_elapsed = cpu_start.elapsed();
  println!("  thread sample provider = {:?}", ThreadCpuInstant::provider());
  println!("  thread sample is CPU time = {}", cpu_start.measures_thread_cpu_time());
  println!("  thread CPU elapsed = {cpu_elapsed:?} (product = {product})");
}
