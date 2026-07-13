#!/usr/bin/env node

import { spawn } from "node:child_process";
import { readFile, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { sep, resolve } from "node:path";
import { createInterface } from "node:readline";

const [generatedInput, chromiumPath] = process.argv.slice(2);
if (generatedInput === undefined || chromiumPath === undefined) {
  throw new Error("usage: run-browser-host-runtime.mjs <generated-dir> <chromium>");
}
const generatedDir = resolve(generatedInput);
const moduleName = "tach_host_runtime_speed.js";

const indexSource = `<!doctype html>
<meta charset="utf-8">
<script type="module">
  globalThis.tachResultPromise = new Promise((resolve, reject) => {
    const worker = new Worker("/worker.mjs", { type: "module" });
    worker.onmessage = ({ data }) => {
      worker.terminate();
      if (data.ok) resolve(data.value);
      else reject(new Error(data.error));
    };
    worker.onerror = ({ message }) => reject(new Error(message));
  });
</script>`;

const workerSource = `
import init, { run } from "/${moduleName}";
try {
  const sibling = new Worker("/sibling.mjs", { type: "module" });
  await new Promise((resolve, reject) => {
    sibling.onmessage = ({ data }) => data === "ready" && resolve();
    sibling.onerror = ({ message }) => reject(new Error(message));
  });
  globalThis.tachBrowserSiblingWorker = sibling;
  await init();
  postMessage({ ok: true, value: run() });
} catch (error) {
  postMessage({ ok: false, error: String(error?.stack ?? error) });
}
`;

const siblingSource = `
postMessage("ready");
self.onmessage = ({ data }) => {
  const signal = new Int32Array(data.signal);
  Atomics.store(signal, 0, 1);
  Atomics.notify(signal, 0);
  const start = performance.now();
  let state = 0;
  while (performance.now() - start < data.millis) {
    state = Math.imul(state, 1664525) + 1013904223;
  }
  Atomics.store(signal, 0, 2);
  Atomics.notify(signal, 0);
};
`;

const headers = {
  "Cache-Control": "no-store",
  "Cross-Origin-Embedder-Policy": "require-corp",
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Resource-Policy": "same-origin",
};

const server = createServer(async (request, response) => {
  try {
    const pathname = new URL(request.url ?? "/", "http://localhost").pathname;
    if (pathname === "/") {
      response.writeHead(200, { ...headers, "Content-Type": "text/html; charset=utf-8" });
      response.end(indexSource);
      return;
    }
    if (pathname === "/worker.mjs") {
      response.writeHead(200, { ...headers, "Content-Type": "text/javascript; charset=utf-8" });
      response.end(workerSource);
      return;
    }
    if (pathname === "/sibling.mjs") {
      response.writeHead(200, { ...headers, "Content-Type": "text/javascript; charset=utf-8" });
      response.end(siblingSource);
      return;
    }
    const relative = decodeURIComponent(pathname).replace(/^\/+/, "");
    const filePath = resolve(generatedDir, relative);
    if (!filePath.startsWith(generatedDir + sep)) {
      response.writeHead(404, headers);
      response.end("not found");
      return;
    }
    const body = await readFile(filePath);
    const type = relative.endsWith(".wasm")
      ? "application/wasm"
      : "text/javascript; charset=utf-8";
    response.writeHead(200, { ...headers, "Content-Type": type });
    response.end(body);
  } catch (error) {
    response.writeHead(500, headers);
    response.end(String(error));
  }
});

await new Promise((resolveListen, rejectListen) => {
  server.once("error", rejectListen);
  server.listen(0, "127.0.0.1", resolveListen);
});
const address = server.address();
if (address === null || typeof address === "string") {
  throw new Error("browser evidence server did not bind a TCP port");
}
const pageUrl = `http://127.0.0.1:${address.port}/`;

const profileDir = "/tmp/tach-browser-profile-" + process.pid;
const browser = spawn(chromiumPath, [
  "--headless",
  "--no-sandbox",
  "--disable-gpu",
  "--disable-background-timer-throttling",
  "--disable-backgrounding-occluded-windows",
  "--disable-renderer-backgrounding",
  "--no-first-run",
  "--remote-debugging-port=0",
  "--user-data-dir=" + profileDir,
  "about:blank",
], { stdio: ["ignore", "ignore", "pipe"] });

let stderr = "";
const lines = createInterface({ input: browser.stderr });
const browserWebSocket = await Promise.race([
  new Promise((resolveSocket, rejectSocket) => {
    lines.on("line", line => {
      stderr += line + "\n";
      const match = line.match(/DevTools listening on (ws:\/\/\S+)/);
      if (match !== null) resolveSocket(match[1]);
    });
    browser.once("exit", code => rejectSocket(
      new Error(`Chromium exited before DevTools startup (${code})\n${stderr}`),
    ));
  }),
  new Promise((_, rejectTimeout) => setTimeout(
    () => rejectTimeout(new Error(`Chromium DevTools startup timed out\n${stderr}`)),
    30_000,
  )),
]);

let socket;
try {
  const debuggerEndpoint = new URL(browserWebSocket);
  const pageResponse = await fetch(
    `http://${debuggerEndpoint.host}/json/new?${encodeURIComponent(pageUrl)}`,
    { method: "PUT" },
  );
  if (!pageResponse.ok) {
    throw new Error(`Chromium target creation failed: ${pageResponse.status}`);
  }
  const page = await pageResponse.json();
  socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolveOpen, rejectOpen) => {
    socket.addEventListener("open", resolveOpen, { once: true });
    socket.addEventListener("error", rejectOpen, { once: true });
  });

  let nextId = 1;
  const pending = new Map();
  socket.addEventListener("message", event => {
    const message = JSON.parse(event.data);
    if (message.id === undefined) return;
    const waiter = pending.get(message.id);
    if (waiter === undefined) return;
    pending.delete(message.id);
    if (message.error === undefined) waiter.resolve(message.result);
    else waiter.reject(new Error(JSON.stringify(message.error)));
  });
  const command = (method, params = {}) => new Promise((resolveCommand, rejectCommand) => {
    const id = nextId++;
    pending.set(id, { resolve: resolveCommand, reject: rejectCommand });
    socket.send(JSON.stringify({ id, method, params }));
  });

  await command("Runtime.enable");
  const evaluation = await Promise.race([
    command("Runtime.evaluate", {
      expression: `new Promise((resolve, reject) => {
        const deadline = Date.now() + 120000;
        const wait = () => {
          if (globalThis.tachResultPromise !== undefined) {
            globalThis.tachResultPromise.then(resolve, reject);
          } else if (Date.now() < deadline) {
            setTimeout(wait, 10);
          } else {
            reject(new Error("browser benchmark promise was never installed"));
          }
        };
        wait();
      })`,
      awaitPromise: true,
      returnByValue: true,
    }),
    new Promise((_, rejectTimeout) => setTimeout(
      () => rejectTimeout(new Error(`browser benchmark timed out\n${stderr}`)),
      150_000,
    )),
  ]);
  if (evaluation.exceptionDetails !== undefined) {
    throw new Error(JSON.stringify(evaluation.exceptionDetails));
  }
  const value = evaluation.result?.value;
  if (typeof value !== "string") {
    throw new Error("browser benchmark returned no serialized observation");
  }
  process.stdout.write(value + "\n");
} finally {
  if (socket !== undefined) socket.close();
  browser.kill("SIGTERM");
  lines.close();
  server.closeAllConnections();
  await new Promise(resolveClose => server.close(resolveClose));
  await rm(profileDir, { force: true, recursive: true });
}
