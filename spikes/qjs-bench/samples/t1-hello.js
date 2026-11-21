// T1 done-when: await a timer and log, plus a smoke test of every prelude global.
const started = performance.now();

(async () => {
  console.log("hello from QuickJS inside wasmi");

  const bytes = new TextEncoder().encode("héllo, wörld");
  console.log("utf8 round trip:", new TextDecoder().decode(bytes), `(${bytes.length} bytes)`);

  const random = crypto.getRandomValues(new Uint8Array(16));
  console.log("random bytes:", random.length, "non-zero:", random.some((byte) => byte !== 0));

  await new Promise((resolve) => setTimeout(resolve, 50));
  console.log(`setTimeout(50) resolved after ${(performance.now() - started).toFixed(1)} ms`);

  let ticks = 0;
  await new Promise((resolve) => {
    const id = setInterval(() => {
      ticks += 1;
      if (ticks === 3) {
        clearInterval(id);
        resolve();
      }
    }, 10);
  });
  console.log("setInterval ticked", ticks, "times");

  const echoed = new Promise((resolve) => {
    const unsubscribe = __truapi_frames.subscribe((frame) => {
      unsubscribe();
      resolve(frame);
    });
  });
  __truapi_frames.send(new Uint8Array([1, 2, 3, 4]));
  const frame = await echoed;
  console.log("frame echoed back:", Array.from(frame).join(","));

  console.log("raw host hidden:", typeof __host === "undefined");
  queueMicrotask(() => console.log("microtask ran"));
})().catch((error) => console.error(error));
