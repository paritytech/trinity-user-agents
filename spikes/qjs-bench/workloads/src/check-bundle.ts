// T3 done-when: every workload wrapper runs under Bun, and the worker bundle
// replays the corpus byte for byte. One process per workload, as an engine
// evaluates one file.
import { WORKLOADS } from "./workload-table";

let failed = false;
for (const name of Object.keys(WORKLOADS)) {
  const run = Bun.spawnSync(["bun", new URL("run-bundle.ts", import.meta.url).pathname, name], { stderr: "pipe" });
  const output = `${run.stdout.toString()}${run.stderr.toString()}`.trim();
  console.log(`${run.exitCode === 0 ? "pass" : "FAIL"} ${name}: ${output}`);
  failed ||= run.exitCode !== 0;
}
process.exit(failed ? 1 : 0);
