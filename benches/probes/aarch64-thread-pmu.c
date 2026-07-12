#define _GNU_SOURCE

#include <errno.h>
#include <inttypes.h>
#include <linux/perf_event.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

enum {
  batches = 9,
  reads_per_batch = 4096,
  warmup_reads = 65536,
};

struct mapped_event {
  int fd;
  size_t page_size;
  struct perf_event_mmap_page *page;
};

static uint64_t native_thread_ns(void) {
  struct timespec value;
  if (syscall(SYS_clock_gettime, CLOCK_THREAD_CPUTIME_ID, &value) != 0) {
    perror("clock_gettime thread");
    exit(2);
  }
  return (uint64_t)value.tv_sec * UINT64_C(1000000000) + (uint64_t)value.tv_nsec;
}

static uint64_t monotonic_raw_ns(void) {
  struct timespec value;
  if (syscall(SYS_clock_gettime, CLOCK_MONOTONIC_RAW, &value) != 0) {
    perror("clock_gettime raw");
    exit(2);
  }
  return (uint64_t)value.tv_sec * UINT64_C(1000000000) + (uint64_t)value.tv_nsec;
}

static struct mapped_event open_event(uint32_t type, uint64_t config, uint64_t config1) {
  struct perf_event_attr attr;
  memset(&attr, 0, sizeof(attr));
  attr.type = type;
  attr.size = sizeof(attr);
  attr.config = config;
  attr.config1 = config1;
  attr.pinned = 1;
  attr.exclude_hv = 1;

  int fd = (int)syscall(SYS_perf_event_open, &attr, 0, -1, -1, PERF_FLAG_FD_CLOEXEC);
  if (fd < 0) {
    fprintf(
      stderr,
      "perf_event_open(type=%u config=%" PRIu64 " config1=%" PRIu64 "): %s\n",
      type,
      config,
      config1,
      strerror(errno)
    );
    return (struct mapped_event){.fd = -1};
  }

  long raw_page_size = sysconf(_SC_PAGESIZE);
  if (raw_page_size <= 0) {
    close(fd);
    return (struct mapped_event){.fd = -1};
  }
  size_t page_size = (size_t)raw_page_size;
  void *mapping = mmap(NULL, page_size, PROT_READ, MAP_SHARED, fd, 0);
  if (mapping == MAP_FAILED) {
    fprintf(stderr, "mmap perf event: %s\n", strerror(errno));
    close(fd);
    return (struct mapped_event){.fd = -1};
  }
  return (struct mapped_event){.fd = fd, .page_size = page_size, .page = mapping};
}

static void close_event(struct mapped_event *event) {
  if (event->fd < 0) {
    return;
  }
  munmap(event->page, event->page_size);
  close(event->fd);
  event->fd = -1;
}

static inline uint64_t read_cntvct(void) {
  uint64_t value;
  __asm__ volatile("isb\n\tmrs %0, cntvct_el0\n\tisb" : "=r"(value) : : "memory");
  return value;
}

static inline uint64_t read_pmccntr(void) {
  uint64_t value;
  __asm__ volatile("isb\n\tmrs %0, pmccntr_el0\n\tisb" : "=r"(value) : : "memory");
  return value;
}

static inline int64_t sign_extend(uint64_t value, uint16_t width) {
  if (width == 0 || width > 64) {
    return 0;
  }
  if (width == 64) {
    return (int64_t)value;
  }
  unsigned shift = 64u - width;
  return ((int64_t)(value << shift)) >> shift;
}

static inline bool read_task_clock(const struct mapped_event *event, uint64_t *out) {
  const struct perf_event_mmap_page *page = event->page;
  for (unsigned attempt = 0; attempt < 16; ++attempt) {
    uint32_t sequence = __atomic_load_n(&page->lock, __ATOMIC_ACQUIRE);
    if (sequence & 1u) {
      continue;
    }
    uint32_t index = __atomic_load_n(&page->index, __ATOMIC_RELAXED);
    uint64_t enabled = __atomic_load_n(&page->time_enabled, __ATOMIC_RELAXED);
    uint64_t running = __atomic_load_n(&page->time_running, __ATOMIC_RELAXED);
    uint64_t capabilities = __atomic_load_n(&page->capabilities, __ATOMIC_RELAXED);
    uint16_t shift = __atomic_load_n(&page->time_shift, __ATOMIC_RELAXED);
    uint32_t mult = __atomic_load_n(&page->time_mult, __ATOMIC_RELAXED);
    uint64_t offset = __atomic_load_n(&page->time_offset, __ATOMIC_RELAXED);
    uint64_t time_cycles = __atomic_load_n(&page->time_cycles, __ATOMIC_RELAXED);
    uint64_t time_mask = __atomic_load_n(&page->time_mask, __ATOMIC_RELAXED);
    uint64_t cycle = read_cntvct();
    atomic_thread_fence(memory_order_acquire);
    uint32_t after = __atomic_load_n(&page->lock, __ATOMIC_RELAXED);
    if (sequence != after || (after & 1u)) {
      continue;
    }
    if (
      index != 0 || enabled != running || !(capabilities & (UINT64_C(1) << 3)) || shift >= 64 ||
      mult == 0
    ) {
      return false;
    }
    if (capabilities & (UINT64_C(1) << 5)) {
      cycle = time_cycles + ((cycle - time_cycles) & time_mask);
    }
    uint64_t remainder_mask = shift == 0 ? 0 : (UINT64_C(1) << shift) - 1;
    uint64_t converted = (cycle >> shift) * mult + ((cycle & remainder_mask) * mult >> shift);
    *out = enabled + offset + converted;
    return true;
  }
  return false;
}

static inline bool read_pmccntr_count(const struct mapped_event *event, uint64_t *out) {
  const struct perf_event_mmap_page *page = event->page;
  for (unsigned attempt = 0; attempt < 16; ++attempt) {
    uint32_t sequence = __atomic_load_n(&page->lock, __ATOMIC_ACQUIRE);
    if (sequence & 1u) {
      continue;
    }
    uint32_t index = __atomic_load_n(&page->index, __ATOMIC_RELAXED);
    int64_t offset = __atomic_load_n(&page->offset, __ATOMIC_RELAXED);
    uint64_t capabilities = __atomic_load_n(&page->capabilities, __ATOMIC_RELAXED);
    uint16_t width = __atomic_load_n(&page->pmc_width, __ATOMIC_RELAXED);
    uint64_t enabled = __atomic_load_n(&page->time_enabled, __ATOMIC_RELAXED);
    uint64_t running = __atomic_load_n(&page->time_running, __ATOMIC_RELAXED);
    uint64_t raw = read_pmccntr();
    atomic_thread_fence(memory_order_acquire);
    uint32_t after = __atomic_load_n(&page->lock, __ATOMIC_RELAXED);
    if (sequence != after || (after & 1u)) {
      continue;
    }
    if (
      index != 32 || !(capabilities & (UINT64_C(1) << 2)) || width != 64 || enabled != running
    ) {
      return false;
    }
    *out = (uint64_t)(offset + sign_extend(raw, width));
    return true;
  }
  return false;
}

static inline bool read_pmccntr_ns(const struct mapped_event *event, uint64_t *out) {
  uint64_t cycles;
  if (!read_pmccntr_count(event, &cycles)) {
    return false;
  }
  /* Graviton3's fixed 2.6 GHz rate is exactly 13/5 cycles per nanosecond. */
  *out = (uint64_t)(((__uint128_t)cycles * 5u) / 13u);
  return true;
}

typedef bool (*reader)(const struct mapped_event *, uint64_t *);

static uint64_t measure(const struct mapped_event *event, reader read) {
  uint64_t value = 0;
  uint64_t start = monotonic_raw_ns();
  for (unsigned i = 0; i < reads_per_batch; ++i) {
    if (!read(event, &value)) {
      fprintf(stderr, "candidate became unreadable\n");
      exit(3);
    }
    __asm__ volatile("" : "+r"(value));
  }
  return monotonic_raw_ns() - start;
}

static int compare_u64(const void *left, const void *right) {
  uint64_t a = *(const uint64_t *)left;
  uint64_t b = *(const uint64_t *)right;
  return (a > b) - (a < b);
}

static void report(const char *name, uint64_t values[batches]) {
  uint64_t sorted[batches];
  memcpy(sorted, values, sizeof(sorted));
  qsort(sorted, batches, sizeof(sorted[0]), compare_u64);
  printf("%-22s median %.3f ns/read batches", name, (double)sorted[batches / 2] / reads_per_batch);
  for (unsigned i = 0; i < batches; ++i) {
    printf(" %" PRIu64, values[i]);
  }
  putchar('\n');
}

static void busy_until_thread_ns(uint64_t delta) {
  uint64_t start = native_thread_ns();
  volatile uint64_t sink = 1;
  while (native_thread_ns() - start < delta) {
    for (unsigned i = 0; i < 10000; ++i) {
      sink = sink * UINT64_C(6364136223846793005) + i;
    }
  }
  __asm__ volatile("" : : "r"(sink) : "memory");
}

static void pin_to_cpu(unsigned cpu) {
  cpu_set_t affinity;
  CPU_ZERO(&affinity);
  CPU_SET(cpu, &affinity);
  if (sched_setaffinity(0, sizeof(affinity), &affinity) != 0) {
    perror("sched_setaffinity");
    exit(2);
  }
}

int main(void) {
  pin_to_cpu(0);
  struct mapped_event task = open_event(PERF_TYPE_SOFTWARE, PERF_COUNT_SW_TASK_CLOCK, 0);
  struct mapped_event cycles = open_event(PERF_TYPE_HARDWARE, PERF_COUNT_HW_CPU_CYCLES, 3);
  if (task.fd < 0 || cycles.fd < 0) {
    close_event(&task);
    close_event(&cycles);
    return 4;
  }
  printf(
    "task page: index=%u caps=%#" PRIx64 " shift=%u mult=%u mask=%#" PRIx64 "\n",
    task.page->index,
    (uint64_t)task.page->capabilities,
    task.page->time_shift,
    task.page->time_mult,
    (uint64_t)task.page->time_mask
  );
  printf(
    "pmu page: index=%u caps=%#" PRIx64 " width=%u enabled=%" PRIu64 " running=%" PRIu64
    "\n",
    cycles.page->index,
    (uint64_t)cycles.page->capabilities,
    cycles.page->pmc_width,
    (uint64_t)cycles.page->time_enabled,
    (uint64_t)cycles.page->time_running
  );

  uint64_t task_value = 0;
  uint64_t cycle_value = 0;
  uint64_t cycle_ns = 0;
  if (
    !read_task_clock(&task, &task_value) || !read_pmccntr_count(&cycles, &cycle_value) ||
    !read_pmccntr_ns(&cycles, &cycle_ns)
  ) {
    fprintf(stderr, "mapped provider not readable\n");
    close_event(&task);
    close_event(&cycles);
    return 5;
  }
  for (unsigned i = 0; i < warmup_reads; ++i) {
    read_task_clock(&task, &task_value);
    read_pmccntr_ns(&cycles, &cycle_ns);
  }

  uint64_t task_samples[batches] = {0};
  uint64_t pmu_samples[batches] = {0};
  for (unsigned sample = 0; sample < batches; ++sample) {
    if (sample & 1u) {
      pmu_samples[sample] = measure(&cycles, read_pmccntr_ns);
      task_samples[sample] = measure(&task, read_task_clock);
    } else {
      task_samples[sample] = measure(&task, read_task_clock);
      pmu_samples[sample] = measure(&cycles, read_pmccntr_ns);
    }
  }
  report("task-clock/CNTVCT", task_samples);
  report("PMCCNTR cycles->ns", pmu_samples);

  uint64_t native_start = native_thread_ns();
  uint64_t task_start = 0;
  uint64_t pmu_start = 0;
  read_task_clock(&task, &task_start);
  read_pmccntr_ns(&cycles, &pmu_start);
  busy_until_thread_ns(UINT64_C(50000000));
  uint64_t native_end = native_thread_ns();
  uint64_t task_end = 0;
  uint64_t pmu_end = 0;
  read_task_clock(&task, &task_end);
  read_pmccntr_ns(&cycles, &pmu_end);
  printf(
    "busy deltas native=%" PRIu64 " task=%" PRIu64 " pmu=%" PRIu64 "\n",
    native_end - native_start,
    task_end - task_start,
    pmu_end - pmu_start
  );

  native_start = native_thread_ns();
  read_task_clock(&task, &task_start);
  read_pmccntr_ns(&cycles, &pmu_start);
  usleep(50000);
  native_end = native_thread_ns();
  read_task_clock(&task, &task_end);
  read_pmccntr_ns(&cycles, &pmu_end);
  printf(
    "sleep deltas native=%" PRIu64 " task=%" PRIu64 " pmu=%" PRIu64 "\n",
    native_end - native_start,
    task_end - task_start,
    pmu_end - pmu_start
  );

  if (sysconf(_SC_NPROCESSORS_ONLN) > 1) {
    native_start = native_thread_ns();
    read_task_clock(&task, &task_start);
    read_pmccntr_ns(&cycles, &pmu_start);
    pin_to_cpu(1);
    busy_until_thread_ns(UINT64_C(25000000));
    pin_to_cpu(0);
    native_end = native_thread_ns();
    read_task_clock(&task, &task_end);
    read_pmccntr_ns(&cycles, &pmu_end);
    printf(
      "migration deltas native=%" PRIu64 " task=%" PRIu64 " pmu=%" PRIu64 "\n",
      native_end - native_start,
      task_end - task_start,
      pmu_end - pmu_start
    );
  }

  close_event(&task);
  close_event(&cycles);
  return 0;
}
