// Entry of workloads.js, the one script every engine evaluates.
import { startBenchWorker } from "./bench-worker";
import { frameProvider } from "./engine-globals";
import { runMicro } from "./micro";

const config = globalThis.__BENCH ?? { workload: "worker" };
if (config.workload === "worker") {
  startBenchWorker(frameProvider(globalThis.__truapi_frames));
} else {
  runMicro(config).catch((error: unknown) => console.error("workload failed", error));
}
