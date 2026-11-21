import type { BenchConfig } from "./engine-globals";

/** Every workload wrapper the build emits as dist/w-<name>.js. */
export const WORKLOADS: Record<string, BenchConfig> = {
  worker: { workload: "worker" },
  codec: { workload: "codec", iterations: 2000 },
  rxjs: { workload: "rxjs", events: 10_000 },
  "pingpong-64": { workload: "pingpong", bytes: 64, iterations: 500 },
  "pingpong-4k": { workload: "pingpong", bytes: 4096, iterations: 500 },
};
