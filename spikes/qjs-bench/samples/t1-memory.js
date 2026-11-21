// Expected: QuickJS's own heap limit throws "out of memory" inside JS,
// before the host's linear-memory cap is reached.
const chunks = [];
const started = performance.now();
try {
  for (;;) {
    chunks.push(new Uint8Array(1 << 20));
    if (chunks.length % 4 === 0) console.log("allocated MiB:", chunks.length, "at", (performance.now() - started).toFixed(0), "ms");
  }
} catch (error) {
  console.log("caught:", String(error), "after", chunks.length, "MiB");
}
