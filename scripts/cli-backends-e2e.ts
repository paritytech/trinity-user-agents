/**
 * Runs the truapi-host CLI against Paseo Next v2 on both chain backends, `rpc`
 * and `light-client`, and compares what each one serves:
 *
 *   bun scripts/cli-backends-e2e.ts [--session <name>] [--no-browser]
 *
 * For each backend it runs the battery's signing-host and pairing-host phases
 * and the playground diagnosis under `truapi-host dev`, then checks the routing
 * rules the CLI enforces and runs the crate's live chain tests. It fails when a
 * Chain, Statement Store or Preimage example fails on either backend, when the
 * backends disagree on one, or when a check fails. Reports and logs land in
 * target/cli-backends/.
 *
 * `--session <name>` creates or selects the signing session the hosts use;
 * without it they use the remembered one. `--no-browser` skips the diagnosis,
 * which needs ports 3000 and 9955 free and Playwright's Chromium (or
 * PLAYWRIGHT_CHANNEL=chrome).
 */
import { spawn } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  copyFileSync,
  writeFileSync,
} from "node:fs";
import { createConnection } from "node:net";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const out = resolve(root, "target/cli-backends");
const host = resolve(root, "target/debug/truapi-host");
const backends = ["rpc", "light-client"] as const;
type Backend = (typeof backends)[number];

/** Services whose examples reach the chains, so both backends must pass them. */
const chainServices = ["Chain", "Statement Store", "Preimage"];
const peopleUrl = "wss://paseo-people-next-system-rpc.polkadot.io";
const assetHubUrl = "wss://paseo-asset-hub-next-rpc.polkadot.io";

interface Line {
  at: number;
  text: string;
}

interface Run {
  code: number;
  lines: Line[];
}

/** Runs a command to completion, keeping every output line with its arrival time. */
function run(
  command: string,
  args: string[],
  env: Record<string, string> = {},
  cwd = root,
): Promise<Run> {
  return new Promise((done, fail) => {
    const child = spawn(command, args, {
      cwd,
      env: { ...process.env, ...env },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const lines: Line[] = [];
    const readers = [child.stdout, child.stderr].map((stream) => {
      const reader = { partial: "" };
      stream.on("data", (chunk: Buffer) => {
        const parts = (reader.partial + chunk.toString()).split("\n");
        reader.partial = parts.pop() ?? "";
        for (const text of parts) lines.push({ at: performance.now(), text });
      });
      return reader;
    });
    child.on("error", fail);
    // battery.sh leaves its watchdog's `sleep` holding these pipes for minutes
    // after it exits, so a run ends with its process, not with its pipes.
    child.on("exit", (code) =>
      setTimeout(() => {
        child.stdout.destroy();
        child.stderr.destroy();
        for (const { partial } of readers) {
          if (partial) lines.push({ at: performance.now(), text: partial });
        }
        done({ code: code ?? 1, lines });
      }, 500),
    );
  });
}

function save(run: Run, name: string): void {
  writeFileSync(
    resolve(out, `${name}.log`),
    run.lines.map((line) => line.text).join("\n"),
  );
}

/** Pass or fail for every example in a diagnosis report. */
function statuses(report: string): Map<string, boolean> {
  const rows = new Map<string, boolean>();
  for (const line of readFileSync(report, "utf8").split("\n")) {
    const row = /^\| `([^`]+)` \| (✅|❌) \|/.exec(line);
    if (row) rows.set(row[1], row[2] === "✅");
  }
  return rows;
}

function portInUse(port: number): Promise<boolean> {
  return new Promise((done) => {
    const socket = createConnection({ port, host: "127.0.0.1" });
    socket.once("connect", () => (socket.destroy(), done(true)));
    socket.once("error", () => (socket.destroy(), done(false)));
  });
}

const failures: string[] = [];
function check(passed: boolean, description: string): void {
  console.log(`${passed ? "✓" : "✗"} ${description}`);
  if (!passed) failures.push(description);
}

const args = process.argv.slice(2);
const session = args.includes("--session")
  ? args[args.indexOf("--session") + 1]
  : undefined;
const browser = !args.includes("--no-browser");
mkdirSync(out, { recursive: true });

console.log("Building truapi-host");
const build = await run("cargo", ["build", "-p", "truapi-host-cli"]);
if (build.code !== 0)
  throw new Error(
    build.lines
      .slice(-20)
      .map((line) => line.text)
      .join("\n"),
  );
if (session) {
  const selected = await run(host, [
    "signing-host",
    "exec",
    `/session ${session}`,
  ]);
  if (selected.code !== 0)
    throw new Error(selected.lines.map((line) => line.text).join("\n"));
}

type Phase = "signing" | "pairing" | "dev";
const reports = new Map<string, Map<string, boolean>>();
const logs = new Map<string, Run>();

async function battery(
  phase: "signing" | "pairing",
  backend: Backend,
): Promise<void> {
  const report = resolve(out, `${phase}-${backend}.md`);
  rmSync(report, { force: true });
  const env: Record<string, string> = {
    TRUAPI_CHAIN_BACKEND: backend,
    TRUAPI_BATTERY_REPORT_PATH: report,
  };
  // Shows the pool statuses an allowance submission sees, for the dropped-claim check.
  if (phase === "signing" && backend === "light-client")
    env.RUST_LOG = "info,truapi=debug";
  const result = await run("scripts/battery.sh", [`--${phase}-host`], env);
  save(result, `${phase}-${backend}`);
  logs.set(`${phase}-${backend}`, result);
  // The battery exits non-zero for examples outside its baseline, so the report is the result.
  if (existsSync(report)) reports.set(`${phase}-${backend}`, statuses(report));
  else failures.push(`${phase} battery on ${backend} wrote no report`);
}

async function diagnosis(backend: Backend): Promise<void> {
  const report = resolve(
    root,
    "playground/test-results/e2e-cli/diagnosis-report.md",
  );
  rmSync(report, { force: true });
  const result = await run(
    "bun",
    ["tests/e2e/cli-diagnosis.ts"],
    { TRUAPI_CHAIN_BACKEND: backend, TRUAPI_HOST_BIN: host },
    resolve(root, "playground"),
  );
  save(result, `dev-${backend}`);
  logs.set(`dev-${backend}`, result);
  if (result.code === 0 && existsSync(report)) {
    copyFileSync(report, resolve(out, `dev-${backend}.md`));
    reports.set(`dev-${backend}`, statuses(report));
  } else
    failures.push(
      `dev diagnosis on ${backend} failed (see target/cli-backends/dev-${backend}.log)`,
    );
}

const phases: Phase[] = ["signing", "pairing"];
if (browser) {
  // A running stack would be reused by the diagnosis, and it may be another build.
  if ((await portInUse(3000)) || (await portInUse(9955))) {
    throw new Error(
      "ports 3000 or 9955 are in use; stop that dev server or pass --no-browser",
    );
  }
  phases.push("dev");
}
for (const backend of backends) {
  for (const phase of phases) {
    console.log(`Running ${phase} on ${backend}`);
    if (phase === "dev") await diagnosis(backend);
    else await battery(phase, backend);
  }
}

console.log("\nResults");
for (const phase of phases) {
  const rpc = reports.get(`${phase}-rpc`);
  const light = reports.get(`${phase}-light-client`);
  if (!rpc || !light) continue;
  const passed = (rows: Map<string, boolean>) =>
    [...rows.values()].filter(Boolean).length;
  console.log(
    `${phase}: rpc ${passed(rpc)}/${rpc.size} passed, light-client ${passed(light)}/${light.size} passed`,
  );
  for (const [example, rpcPassed] of rpc) {
    const lightPassed = light.get(example);
    const chainFacing = chainServices.some((service) =>
      example.startsWith(`${service}/`),
    );
    if (chainFacing && !(rpcPassed && lightPassed)) {
      failures.push(
        `${phase}: ${example} failed (rpc ${rpcPassed ? "passed" : "failed"}, light-client ${lightPassed ? "passed" : "failed"})`,
      );
    } else if (rpcPassed !== lightPassed) {
      console.log(
        `  differs: ${example} (rpc ${rpcPassed ? "passed" : "failed"}, light-client ${lightPassed ? "passed" : "failed"})`,
      );
    }
  }
}

console.log("\nChecks");
const smoldotLine = /legacy JSON-RPC API|protocolId|initialization complete/;
check(
  phases.every(
    (phase) =>
      !logs
        .get(`${phase}-rpc`)
        ?.lines.some((line) => smoldotLine.test(line.text)),
  ),
  "RPC runs never start the light client",
);

const lightSigning = logs.get("signing-light-client")?.lines ?? [];
const dropped = lightSigning.findIndex((line) =>
  line.text.includes("status=Dropped"),
);
if (dropped < 0)
  console.log(
    "- no allowance transaction was dropped on the light client, so the dropped-status check did not run",
  );
else {
  const failed = lightSigning
    .slice(dropped)
    .find((line) => line.text.includes("reason=extrinsic dropped"));
  check(
    failed !== undefined && failed.at - lightSigning[dropped].at < 1000,
    "a dropped allowance transaction fails as soon as the pool reports it",
  );
}

// The host resolves the network before it runs a command, so `/session` shows
// whether a routing is accepted.
const resolveWith = (env: Record<string, string>) =>
  run(host, ["signing-host", "exec", "/session"], env);
const atAssetHub = await resolveWith({
  HOST_CLI_PEOPLE_WS: assetHubUrl,
  TRUAPI_CHAIN_BACKEND: "rpc",
});
check(
  atAssetHub.code !== 0 &&
    atAssetHub.lines.some((line) => line.text.includes("reports genesis")),
  "People pointed at Asset Hub's URL is refused",
);
const atPeople = await resolveWith({
  HOST_CLI_PEOPLE_WS: peopleUrl,
  TRUAPI_CHAIN_BACKEND: "rpc",
});
check(atPeople.code === 0, "People pointed at its own URL is accepted");
const withLight = await resolveWith({
  HOST_CLI_PEOPLE_WS: peopleUrl,
  TRUAPI_CHAIN_BACKEND: "light-client",
});
check(
  withLight.code !== 0 &&
    withLight.lines.some((line) =>
      line.text.includes("cannot be used together"),
    ),
  "a chain override is refused with the light client",
);

console.log("Running the live chain tests");
const live = await run("cargo", [
  "test",
  "-p",
  "truapi-host-cli",
  "--bin",
  "truapi-host",
  "reaches_paseo",
  "--",
  "--ignored",
  "--test-threads=1",
]);
save(live, "live-chain-tests");
check(
  live.code === 0,
  "both backends reach every chain, and a light-client chain outlives its connections",
);

if (failures.length > 0) {
  console.log(
    `\n${failures.length} failed:\n${failures.map((failure) => `  - ${failure}`).join("\n")}`,
  );
  process.exit(1);
}
console.log("\nAll checks passed. Reports in target/cli-backends/");
