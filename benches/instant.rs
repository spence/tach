#![allow(clippy::cast_precision_loss)]

use std::hint::black_box;
use std::time::Instant as StdInstant;

use criterion::{Criterion, criterion_group, criterion_main};
use tach::{Instant, FencedInstant, SyncedInstant};

fn bench_now(c: &mut Criterion) {
  // Prime the lazy frequency calibration so it doesn't land in the first
  // measured sample.
  quanta::Instant::now();
  fastant::Instant::now();
  minstant::Instant::now();

  let mut g = c.benchmark_group("Instant::now()");
  g.bench_function("tach", |b| b.iter(|| black_box(Instant::now())));
  g.bench_function("quanta", |b| b.iter(|| black_box(quanta::Instant::now())));
  g.bench_function("fastant", |b| b.iter(|| black_box(fastant::Instant::now())));
  g.bench_function("minstant", |b| b.iter(|| black_box(minstant::Instant::now())));
  g.bench_function("std", |b| b.iter(|| black_box(StdInstant::now())));
  g.finish();
}

fn bench_elapsed(c: &mut Criterion) {
  quanta::Instant::now();
  fastant::Instant::now();
  minstant::Instant::now();

  let mut g = c.benchmark_group("Instant::now() + elapsed()");
  g.bench_function("tach", |b| {
    b.iter(|| {
      let start = Instant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("quanta", |b| {
    b.iter(|| {
      let start = quanta::Instant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("fastant", |b| {
    b.iter(|| {
      let start = fastant::Instant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("minstant", |b| {
    b.iter(|| {
      let start = minstant::Instant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("std", |b| {
    b.iter(|| {
      let start = StdInstant::now();
      black_box(start.elapsed())
    });
  });
  g.finish();
}

fn bench_fenced(c: &mut Criterion) {
  let mut g = c.benchmark_group("Fenced Instant::now()");
  g.bench_function("tach::FencedInstant", |b| {
    b.iter(|| black_box(FencedInstant::now()));
  });
  g.bench_function("tach::FencedInstant (now + elapsed)", |b| {
    b.iter(|| {
      let start = FencedInstant::now();
      black_box(start.elapsed())
    });
  });
  g.bench_function("tach::Instant (unfenced reference)", |b| {
    b.iter(|| black_box(Instant::now()));
  });
  g.bench_function("std::time::Instant", |b| {
    b.iter(|| black_box(StdInstant::now()));
  });
  g.bench_function("std::time::Instant (now + elapsed)", |b| {
    b.iter(|| {
      let start = StdInstant::now();
      black_box(start.elapsed())
    });
  });
  g.finish();
}

// Isolates `elapsed()` alone (one counter read + the subtraction + conversion),
// holding `start` outside the loop so the second `now()` of the combined bench
// doesn't dilute the signal. This is the group that exposes the saturating_sub
// cost most directly.
fn bench_elapsed_only(c: &mut Criterion) {
  let mut g = c.benchmark_group("elapsed() only");
  let tach_start = Instant::now();
  g.bench_function("tach::Instant", |b| {
    b.iter(|| black_box(black_box(tach_start).elapsed()));
  });
  let sync_start = SyncedInstant::now();
  g.bench_function("tach::SyncedInstant", |b| {
    b.iter(|| black_box(black_box(sync_start).elapsed()));
  });
  let fenced_start = FencedInstant::now();
  g.bench_function("tach::FencedInstant", |b| {
    b.iter(|| black_box(black_box(fenced_start).elapsed()));
  });
  let std_start = StdInstant::now();
  g.bench_function("std::time::Instant", |b| {
    b.iter(|| black_box(black_box(std_start).elapsed()));
  });
  g.finish();
}

criterion_group!(benches, bench_now, bench_elapsed, bench_elapsed_only, bench_fenced);
criterion_main!(benches);
