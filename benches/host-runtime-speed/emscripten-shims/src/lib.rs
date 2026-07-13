#![cfg(target_os = "emscripten")]

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
  __em_js__tach_host_benchmark_now_nanos,
  b"()<::>{try{return Number(globalThis.process.hrtime.bigint())}catch(_){return -1}}\0"
);

em_js!(
  __em_js__tach_host_sleep_millis,
  b"(i)<::>{const s=new Int32Array(new SharedArrayBuffer(4));Atomics.wait(s,0,0,i)}\0"
);

em_js!(
  __em_js__tach_host_sibling_work_millis,
  b"(m)<::>{const p=globalThis.process;const w=p.getBuiltinModule(\"node:worker_threads\");const s=new Int32Array(new SharedArrayBuffer(4));const t=new w.Worker(`const{workerData}=require(\"node:worker_threads\");const s=new Int32Array(workerData.signal);Atomics.store(s,0,1);Atomics.notify(s,0);const b=process.hrtime.bigint();const d=BigInt(workerData.millis)*1000000n;let x=0n;while(process.hrtime.bigint()-b<d){x=x*6364136223846793005n+1442695040888963407n;x&=0xffffffffffffffffn}Atomics.store(s,0,2);Atomics.notify(s,0)`,{eval:true,workerData:{signal:s.buffer,millis:m}});while(Atomics.load(s,0)===0)Atomics.wait(s,0,0);while(Atomics.load(s,0)!==2)Atomics.wait(s,0,1);t.unref()}\0"
);

#[link(wasm_import_module = "env")]
unsafe extern "C" {
  fn tach_host_benchmark_now_nanos() -> f64;
  fn tach_host_sleep_millis(millis: u32);
  fn tach_host_sibling_work_millis(millis: u32);
}

#[inline(never)]
pub fn benchmark_now_nanos() -> f64 {
  // SAFETY: the linked shim catches host errors and returns a finite sentinel.
  unsafe { tach_host_benchmark_now_nanos() }
}

#[inline(never)]
pub fn sleep_millis(millis: u32) {
  // SAFETY: the linked shim blocks only the current Node thread.
  unsafe { tach_host_sleep_millis(millis) }
}

#[inline(never)]
pub fn sibling_work_millis(millis: u32) {
  // SAFETY: the linked shim joins an isolated Node worker through shared memory.
  unsafe { tach_host_sibling_work_millis(millis) }
}
