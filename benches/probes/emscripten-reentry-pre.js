(() => {
  const performance = globalThis.performance;
  if (performance === undefined || performance === null || typeof performance.now !== "function") {
    throw new Error("Emscripten reentry probe needs globalThis.performance.now");
  }

  const originalNow = performance.now.bind(performance);
  let armed = true;
  performance.now = function tachEmscriptenReentryNow() {
    const value = originalNow();
    if (armed) {
      armed = false;
      const reenter = Module._tach_emscripten_reentry_now;
      if (typeof reenter !== "function") {
        throw new Error("Emscripten reentry probe export is unavailable");
      }
      reenter();
    }
    return value;
  };
})();
