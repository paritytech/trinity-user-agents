// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// PROBE: does RFC-0010 `AutoSigning` actually silence the host's per-call
// `confirmUserAction`, and is the resulting signature fast enough for
// high-volume signing?
//
// WHY THIS EXISTS
//
// bulletin-deploy submits ONE Bulletin extrinsic per 2 MiB chunk (a 100 MB
// site is ~50 signatures, plus up to 3 retries each). It can only adopt a
// phone-paired host if signing is both unprompted and fast. Two independent
// things must hold:
//
//   1. No prompt per signature. `@parity/truapi`'s AccountClient.signVrf doc
//      comment states the contract -- "local when `AutoSigning` covers the
//      account, otherwise a per-call user confirmation" -- and `AutoSigning`
//      is a first-class `AllocatableResource`. But host-cli's
//      `confirmUserAction` (src/callbacks.ts) routes EVERY review to
//      `presenter.confirm` with no suppression, so whether the core stops
//      invoking the callback is unverified from this side.
//   2. Sub-second signatures. A suppressed prompt is worthless if each
//      signature still round-trips to the phone. At 3s/signature a 50-chunk
//      deploy takes 2.5 minutes of pure signing.
//
// This probe measures both, before/after the grant, and prints a verdict.
//
// HOW IT MEASURES
//
// Baseline first: run the signing ops WITHOUT AutoSigning and count prompts.
// That baseline is what makes a post-grant zero meaningful -- an op that
// errors before reaching the review surface also produces zero prompts, and
// without the baseline the two are indistinguishable. An op that never
// prompted in the baseline is reported INCONCLUSIVE, not PASS.
//
// USAGE
//
//   bun examples/autosigning-probe.ts
//   node --experimental-strip-types examples/autosigning-probe.ts
//
// Needs network egress to Paseo Next V2 and a phone with the Polkadot app to
// scan the QR on first run. State persists in the storage dir (0600), so
// re-runs skip pairing. Delete the directory to force a re-pair.
//
//   PROBE_ROUNDS=3              signing ops per phase, per op kind
//   PROBE_CHAIN=bulletin        bulletin | assethub -- createTransaction target
//   PROBE_MANUAL=1              answer prompts by hand, don't auto-approve
//   PROBE_STORAGE_DIR=<path>    default ~/.dotli-autosigning-probe
//   PROBE_OP_TIMEOUT_MS=200000  per-op backstop; ABOVE the core's 180s SSO
//                               window on purpose, so its diagnosis lands first
//   PROBE_LOG_LEVEL=warn        raise to `debug`/`trace` to see inside a hang
//   PROBE_GRANT_ONLY=1          only ask "does the wallet answer an AutoSigning
//                               allocation?" -- ~3min triage vs. a ~12min run
//   PROBE_GET_ACCOUNT=1         re-test the product-account lookup that the
//                               current mobile build leaves unanswered
//   PROBE_PREIMAGE=1            measure Bulletin writes under the granted
//                               BulletinAllowance. REAL testnet chain writes.
//
// SAFETY: prompts are auto-approved by default, which is exactly the
// anti-pattern host-cli's security model warns about -- so keep it confined to
// what this probe signs. Both ops are inert: `signRaw` over a fixed 8-byte
// string, and `createTransaction` over a `system.remark`, which CONSTRUCTS a
// signed extrinsic and never broadcasts it (broadcast is a separate
// `chain.broadcastTransaction` call this probe does not make). Nothing reaches
// a chain and no funds can move. Run with PROBE_MANUAL=1 to approve by hand.

import { homedir } from "node:os";
import { join } from "node:path";
import { sha256 as sha256Hash } from "@noble/hashes/sha2.js";
import { blake2b } from "@noble/hashes/blake2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import { createClient, createTransport } from "@parity/truapi";
import type {
  AllocatableResource,
  AllocationOutcome,
  ProductAccountId,
} from "@parity/truapi";
import {
  createCliHost,
  createTerminalPresenter,
  explainProductError,
  isProbableSsoTimeout,
  type ConfirmRequest,
  type HostPresenter,
} from "../src/index.js";

// Paseo Next V2, same values as examples/pair.ts (which mirrors
// packages/config/src/network.ts). The assethub/people genesis hashes
// changed with the network refresh, so any probe state paired against the
// old chains is dead: delete the storage dir and re-pair.
const PEOPLE_GENESIS =
  "0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48";
const BULLETIN_GENESIS =
  "0x8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22";
const ASSET_HUB_GENESIS =
  "0x4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a";

// The dotNS identifier the product is addressed by. Worth varying: paseo-next-v2
// declares `tld: "paseo"`, not `.dot`, and this default name is not registered on
// any registry — so a `product-subtree` timeout here may be the phone hanging on
// an unresolvable name rather than a missing handler. Point it at a REGISTERED
// name with the environment's TLD to tell those two apart.
const PRODUCT_ID = process.env.PROBE_PRODUCT_ID ?? "bulletin-deploy-probe.dot";

const ROUNDS = Number(process.env.PROBE_ROUNDS ?? "3");
const MANUAL = process.env.PROBE_MANUAL === "1";
// Deliberately ABOVE the core's own ~180s SSO window. Timing out sooner would
// preempt the core's diagnosis ("SSO response timed out") and replace an
// actionable reason with a bare "TIMEOUT" -- measured: a 45s deadline hid the
// fact that the wallet was simply never answering. This is only a backstop for
// a hang the core does not bound at all; `runPhase` aborts on the first
// wallet-never-answered so a dead wallet costs one wait, not six.
const OP_TIMEOUT_MS = Number(process.env.PROBE_OP_TIMEOUT_MS ?? "200000");
// The core's own diagnosis of a stuck operation is a `tracing` log, not an
// error, so raising this is the only way to see inside a hang.
const LOG_LEVEL = process.env.PROBE_LOG_LEVEL ?? "warn";
// MEASURED dead: `getAccount` SSO-requests `product-subtree` and the current
// mobile build never answers, costing a full 180s for a value nothing else
// depends on. Opt in when re-testing whether the wallet has gained the call.
const GET_ACCOUNT = process.env.PROBE_GET_ACCOUNT === "1";
// Skip the baseline/post-grant comparison and answer only the question that is
// still open: does the wallet ANSWER an AutoSigning allocation request at all?
// Cheap triage for a run where signing is already known to hang.
const GRANT_ONLY = process.env.PROBE_GRANT_ONLY === "1";
// The Bulletin-write measurement. Opt-in because unlike every other op here it
// is a REAL chain write: `preimage.submit` publishes to the Bulletin chain
// (testnet, a few bytes) rather than merely constructing something.
const PREIMAGE = process.env.PROBE_PREIMAGE === "1";
// Skip login/pairing entirely — see the phase-0 note. Pair with a fresh
// PROBE_STORAGE_DIR so no session or allowance key can be inherited.
const NO_LOGIN = process.env.PROBE_NO_LOGIN === "1";
// Fire N writes CONCURRENTLY and compare wall-clock against the sequential
// sweep. bulletin-deploy pipelines chunk submission by nonce; polkadot-bulletin-
// chain#555 says one-at-a-time "never fills the chain's per-block capacity" and
// prescribes wave batching against MaxBlockTransactions. Since the core owns
// submission, a consumer CANNOT add pipelining on top -- so whether the core
// does it decides whether ~50 chunks cost ~50 blocks or ~1.
const CONCURRENCY = Number(process.env.PROBE_CONCURRENCY ?? "0");
// Payload sizes for the sweep, bytes. Default spans the SSO ceiling (509,784):
// two below, one comfortably above. The one above is the decisive case.
const PREIMAGE_SIZES = (process.env.PROBE_PREIMAGE_SIZES ?? "32,102400,614400")
  .split(",")
  .map((s) => Number(s.trim()))
  .filter((n) => Number.isFinite(n) && n > 0);
const STORAGE_DIR =
  process.env.PROBE_STORAGE_DIR ?? join(homedir(), ".dotli-autosigning-probe");
const TX_CHAIN_GENESIS =
  process.env.PROBE_CHAIN === "assethub" ? ASSET_HUB_GENESIS : BULLETIN_GENESIS;

// `System.remark("prob")`: pallet 0, call 0, compact-length 4, then the bytes.
// A real call so the core's decode step cannot be what rejects us -- if this
// errors, the error is about extensions or authorization, not malformed input.
const REMARK_CALL_DATA = "0x00001070726f62";

const ACCOUNT: ProductAccountId = {
  dotNsIdentifier: PRODUCT_ID,
  derivationIndex: { tag: "Index", value: 0 },
};

/**
 * Wraps the real terminal presenter so pairing QR and progress still render,
 * while every `confirm` is counted (and, by default, auto-approved so the
 * probe can measure prompt COUNT without a human in the loop).
 */
interface ProbePresenter extends HostPresenter {
  /** Confirms seen since the last `take()`, with their review titles. */
  take(): string[];
}

function probePresenter(inner: HostPresenter): ProbePresenter {
  let seen: string[] = [];
  return {
    authStateChanged: (state) => inner.authStateChanged(state),
    notify: (text) => inner.notify(text),
    openUrl: (url) => inner.openUrl(url),
    dispose: () => inner.dispose(),
    async confirm(request: ConfirmRequest) {
      seen.push(request.title);
      if (MANUAL) {
        return inner.confirm(request);
      }
      process.stderr.write(`   [probe] auto-approved: ${request.title}\n`);
      return true;
    },
    take() {
      const taken = seen;
      seen = [];
      return taken;
    },
  };
}

interface OpOutcome {
  ok: boolean;
  detail: string;
  /** The raw failure, so the caller can classify it (SSO timeout vs. other). */
  cause?: unknown;
}

interface OpResult extends OpOutcome {
  /** Review titles prompted during this op, in order. */
  titles: string[];
  ms: number;
  /** The wallet was asked and never answered. */
  ssoTimeout: boolean;
}

interface PhaseStats {
  titles: string[];
  latencies: number[];
  failures: string[];
  ssoTimeouts: number;
}

function emptyStats(): PhaseStats {
  return { titles: [], latencies: [], failures: [], ssoTimeouts: 0 };
}

function record(stats: PhaseStats, result: OpResult): void {
  stats.titles.push(...result.titles);
  // Only SUCCESSFUL ops contribute latency. Measured why: a failed op returns
  // when its 180s authority timeout expires, and folding that into the sample
  // dragged a median of ~5s up to ~130s -- describing the timeout, not the
  // signature. Failures are counted separately instead of poisoning the median.
  if (result.ok) {
    stats.latencies.push(result.ms);
  }
  if (result.ssoTimeout) {
    stats.ssoTimeouts += 1;
  }
  if (!result.ok) {
    stats.failures.push(result.detail);
  }
}

/** Distinct titles with counts, e.g. `Sign a message ×3`. */
function summarizeTitles(titles: string[]): string {
  if (titles.length === 0) {
    return "none";
  }
  const counts = new Map<string, number>();
  for (const title of titles) {
    counts.set(title, (counts.get(title) ?? 0) + 1);
  }
  return [...counts].map(([title, n]) => `${title} ×${String(n)}`).join(", ");
}

function fmt(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms.toFixed(0)}ms`;
}

function median(values: number[]): number {
  if (values.length === 0) {
    return Number.NaN;
  }
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? (sorted[mid - 1]! + sorted[mid]!) / 2
    : sorted[mid]!;
}

// ONE instance, shared: the host calls `confirm` on exactly the object whose
// counters the verdict reads. Two instances would make every phase report zero
// prompts and fake a "SUPPRESSED" pass.
const presenter = probePresenter(createTerminalPresenter());

const host = await createCliHost({
  host: { name: "AutoSigning probe", version: "0.1.0" },
  pairing: { deeplinkScheme: "polkadotapp" },
  people: { genesisHash: PEOPLE_GENESIS },
  bulletin: { genesisHash: BULLETIN_GENESIS },
  assetHub: { genesisHash: ASSET_HUB_GENESIS },
  chains: {
    [ASSET_HUB_GENESIS]: {
      name: "Asset Hub",
      role: "AssetHub",
      rpc: "wss://paseo-asset-hub-next-rpc.polkadot.io",
    },
    [PEOPLE_GENESIS]: {
      name: "People",
      role: "People",
      rpc: "wss://paseo-people-next-system-rpc.polkadot.io",
    },
    [BULLETIN_GENESIS]: {
      name: "Bulletin",
      role: "Bulletin",
      rpc: "wss://paseo-bulletin-next-rpc.polkadot.io",
    },
  },
  network: "paseo-next-v2",
  storageDir: STORAGE_DIR,
  presenter,
  logLevel: LOG_LEVEL,
  log: (line) => {
    process.stderr.write(`   [host] ${line}\n`);
  },
});

const product = host.createProduct({ productId: PRODUCT_ID });
const client = createClient(createTransport(product.provider));

/**
 * Nothing in the core's request surface carries its own deadline, and a read
 * that never settles is a REAL failure mode here -- `getAccount` hangs on a
 * People-chain read for an unregistered product id, and host-cli's README
 * documents the same class of hang for chain-head operations. Without this the
 * probe stalls before it measures anything.
 */
class TimeoutError extends Error {}

async function withTimeout<T>(
  label: string,
  ms: number,
  // PromiseLike, not Promise: neverthrow's ResultAsync is thenable but lacks
  // `catch`/`finally`, so it does not satisfy Promise<T>.
  work: PromiseLike<T>,
): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([
      work,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => {
          reject(
            new TimeoutError(`${label} did not settle in ${String(ms)}ms`),
          );
        }, ms);
      }),
    ]);
  } finally {
    if (timer !== undefined) {
      clearTimeout(timer);
    }
  }
}

/**
 * A timed-out request may still be in flight and may prompt LATER, landing in
 * a subsequent op's counting window. Once that has happened the prompt counts
 * are no longer trustworthy, so track it and say so in the verdict rather than
 * reporting a number we cannot stand behind.
 */
let timedOut = false;

/** Run one op, attributing any prompts it triggered and timing it. */
async function timed(
  label: string,
  run: () => Promise<OpOutcome>,
): Promise<OpResult> {
  presenter.take();
  const started = performance.now();
  let outcome: OpOutcome;
  try {
    outcome = await withTimeout(label, OP_TIMEOUT_MS, run());
  } catch (error) {
    if (error instanceof TimeoutError) {
      timedOut = true;
      outcome = { ok: false, detail: `TIMEOUT after ${OP_TIMEOUT_MS}ms` };
    } else {
      outcome = {
        ok: false,
        detail: explainProductError(error) ?? `threw: ${String(error)}`,
        cause: error,
      };
    }
  }
  const ms = performance.now() - started;
  const titles = presenter.take();
  // The wallet-never-answered case is the one failure that makes continuing
  // pointless, so it is classified rather than lumped in with other errors.
  const ssoTimeout =
    isProbableSsoTimeout(outcome.cause) ||
    outcome.detail.startsWith("TIMEOUT after");
  process.stderr.write(
    `   ${label}: ${String(titles.length)} prompt(s) [${summarizeTitles(titles)}], ` +
      `${ms.toFixed(0)}ms, ${outcome.ok ? "ok" : outcome.detail}\n`,
  );
  return { ...outcome, titles, ms, ssoTimeout };
}

async function signRawOp(): Promise<OpOutcome> {
  const result = await client.signing.signRaw({
    account: ACCOUNT,
    payload: { tag: "Bytes", value: { bytes: "0x70726f62653031" } },
  });
  return result.isOk()
    ? { ok: true, detail: "signed" }
    : {
        ok: false,
        detail: `err: ${JSON.stringify(result.error)}`,
        cause: result.error,
      };
}

async function createTransactionOp(): Promise<OpOutcome> {
  const result = await client.signing.createTransaction({
    signer: ACCOUNT,
    genesisHash: TX_CHAIN_GENESIS,
    callData: REMARK_CALL_DATA,
    // Deliberately empty: this probe measures the REVIEW surface, not
    // extension encoding. If the core rejects for missing extensions, the
    // baseline phase records that as a failure AND as a prompt count, which
    // is what makes the verdict honest -- see the INCONCLUSIVE case.
    extensions: [],
    txExtVersion: 0,
    // A remark names nobody.
    contacts: [],
  });
  return result.isOk()
    ? { ok: true, detail: "constructed" }
    : {
        ok: false,
        detail: `err: ${JSON.stringify(result.error)}`,
        cause: result.error,
      };
}

/**
 * Publish N bytes to the Bulletin chain -- the ONE op here that changes chain
 * state, and the one matching what bulletin-deploy does in bulk.
 *
 * Payload SIZE is the experiment. The SSO channel has a hard ceiling:
 *   host-papp  MAX_SSO_REQUEST_SIZE = 498 * 1024      = 509,952 bytes
 *   minus      STATEMENT_OVERHEAD   = 168             → 509,784 usable
 *   statement-store session.js rejects above that with Error('message too big')
 *
 * So a payload over that ceiling can ONLY succeed if it never traverses SSO --
 * i.e. the core signed locally with the allocated (signing-capable) allowance
 * key. That makes one submission a decisive test of which path is in use, and
 * a size sweep additionally shows whether latency tracks payload size (bytes
 * being transported) or stays flat (chain inclusion dominating).
 *
 * Bytes vary per call: identical payloads share a preimage hash and the chain
 * would dedupe them, flattering both latency and the did-it-store answer.
 */
const SSO_PAYLOAD_CEILING = 498 * 1024 - 168; // 509,784

async function preimageSubmitOp(
  sizeBytes: number,
  nonce: number,
): Promise<OpOutcome> {
  const buf = new Uint8Array(sizeBytes);
  let x = (0x9e3779b9 ^ (nonce * 0x85ebca6b) ^ sizeBytes) >>> 0;
  for (let i = 0; i < buf.length; i++) {
    x ^= x << 13; x >>>= 0;
    x ^= x >>> 17;
    x ^= x << 5; x >>>= 0;
    buf[i] = x & 0xff;
  }
  const hex = `0x${Buffer.from(buf).toString("hex")}`;

  // ADDRESSING TEST. bulletin-deploy pins CIDv1 + codec 0x55 (raw) + sha2-256,
  // because that is what makes a stored chunk resolvable at an IPFS gateway
  // under the CID it computes locally. truapi's submit() takes bare bytes and
  // returns "the preimage key" with no codec/hashing parameter, so which digest
  // the core used is only observable from the value it hands back. sha2-256 ⇒ a
  // CIDv1-raw is derivable and the two systems interoperate; anything else ⇒ the
  // bytes are stored but not addressable the way bulletin-deploy needs.
  const sha256 = bytesToHex(sha256Hash(buf));
  const blake2 = bytesToHex(blake2b(buf, { dkLen: 32 }));

  const result = await client.preimage.submit(hex as `0x${string}`);
  if (result.isOk()) {
    const key = result.value.replace(/^0x/, "").toLowerCase();
    const match =
      key === sha256 ? "sha2-256 ✅ (CIDv1-raw derivable)"
      : key === blake2 ? "blake2-256 ❌ (not the IPFS digest)"
      : "unrecognised digest ❓";
    process.stderr.write(
      `      preimage key : 0x${key.slice(0, 32)}…\n` +
        `      sha2-256     : 0x${sha256.slice(0, 32)}…\n` +
        `      blake2-256   : 0x${blake2.slice(0, 32)}…\n` +
        `      => ${match}\n`,
    );
  }
  return result.isOk()
    ? { ok: true, detail: `published ${result.value.slice(0, 14)}…` }
    : {
        ok: false,
        detail: `err: ${JSON.stringify(result.error)}`,
        cause: result.error,
      };
}

const OPS = [
  { key: "signRaw", run: signRawOp },
  { key: "createTransaction", run: createTransactionOp },
] as const;

type OpKey = (typeof OPS)[number]["key"];

async function runPhase(name: string): Promise<Record<OpKey, PhaseStats>> {
  process.stderr.write(`\n== ${name} ==\n`);
  const stats = {
    signRaw: emptyStats(),
    createTransaction: emptyStats(),
  } satisfies Record<OpKey, PhaseStats>;
  for (let round = 1; round <= ROUNDS; round += 1) {
    for (const op of OPS) {
      const result = await timed(`${op.key} #${round}`, op.run);
      record(stats[op.key], result);
      if (result.ssoTimeout) {
        // Each unanswered request costs ~180s. If the wallet ignored this one
        // it will ignore the rest, and grinding through them buys nothing but
        // wall-clock -- so stop and let the verdict report why.
        process.stderr.write(
          `   -> wallet did not answer; abandoning the rest of "${name}"\n`,
        );
        return stats;
      }
    }
  }
  return stats;
}

process.stderr.write(
  `AutoSigning probe -- product ${PRODUCT_ID}, ${String(ROUNDS)} round(s), ` +
    `tx chain ${TX_CHAIN_GENESIS.slice(0, 10)}…\n` +
    `prompts are ${MANUAL ? "answered manually" : "AUTO-APPROVED"}; ` +
    `state in ${STORAGE_DIR}\n`,
);

// --- Phase 0: login (renders the QR on a cold store) -----------------------
// NO_LOGIN skips it entirely. Point that at a FRESH storage dir and the run has
// no session, no pairing and no cached allowance key — so whether
// `preimage.submit` still works answers a question nothing else can: is a signer
// involved in Bulletin writes at all, or is this the chain's UNSIGNED preimage
// path? Every prior observation (works with the app closed, works with
// BulletinAllowance absent) is explained equally well by either, because an
// allowance key was always cached on disk from an earlier run.
if (NO_LOGIN) {
  process.stderr.write(
    "\n== phase 0: SKIPPED (PROBE_NO_LOGIN=1) ==\n" +
      "   No login, no pairing. With a fresh storage dir there is no session\n" +
      "   and no cached allowance key, so a successful write below cannot have\n" +
      "   been signed by one.\n",
  );
} else {
  process.stderr.write("\n== phase 0: login ==\n");
  try {
    const outcome = await client.account.requestLogin({
      reason: "AutoSigning suppression probe",
    });
    process.stderr.write(`   requestLogin -> ${JSON.stringify(outcome)}\n`);
  } catch (error) {
    process.stderr.write(
      `   login failed: ${explainProductError(error) ?? String(error)}\n`,
    );
    product.dispose();
    host.dispose();
    process.exit(1);
  }
}
presenter.take();

// The session identity, straight off the auth state the core already emitted.
// Free: no chain read, no request. This is the ROOT identity, not the account
// that would sign.
const connected = host.authState();
process.stderr.write(
  `   session identity: ${
    connected?.tag === "Connected" ? connected.value.publicKey : "unknown"
  }\n`,
);

// The PRODUCT account is the one that actually signs, so it -- not the session
// identity -- is what bulletin-deploy's Bulletin authorization preflight needs
// (readAccountAuthorization / ensureAuthorized, src/pool.ts).
//
// Guarded because MEASURED: this does not resolve locally. It issues an SSO
// request to the phone (`action="product-subtree"`) and the wallet never
// answers -- the core gives up after 180s with
//   SSO remote message failed ... reason=SSO response timed out after 180s
// So the product account address is not obtainable from the current mobile
// build at all. That is a finding for B4/B5, but it must not stop the probe:
// B2 is the question worth answering and does not depend on this address.
if (NO_LOGIN) {
  process.stderr.write("   product account:  n/a (no session)\n");
} else if (!GET_ACCOUNT) {
  process.stderr.write(
    "   product account:  SKIPPED (known unanswered; PROBE_GET_ACCOUNT=1 to retest)\n",
  );
} else {
  try {
    const account = await withTimeout(
      "getAccount",
      OP_TIMEOUT_MS,
      client.account.getAccount({ productAccountId: ACCOUNT }),
    );
    process.stderr.write(
      `   product account:  ${
        account.isOk()
          ? account.value.account.publicKey
          : `err: ${JSON.stringify(account.error)}`
      }\n`,
    );
  } catch (error) {
    process.stderr.write(
      `   product account:  UNRESOLVED (${
        error instanceof TimeoutError ? error.message : String(error)
      })\n` +
        "   -> B4/B5 finding: the wallet does not answer 'product-subtree',\n" +
        "      so the signing account's address is not obtainable. Continuing.\n",
    );
  }
}

// --- Warm-up: absorb the one-time account-authority handshake -------------
// MEASURED: the first two signing ops of a fresh session fail with
//   Account authority request timed out after 180s
// and every op after them succeeds in seconds. So there is a one-time
// per-session handshake with the wallet, and letting it land inside phase 1
// charges the baseline for a cost that is not per-signature. Discard the
// result: this call exists to pay that toll, not to be measured.
if (process.env.PROBE_NO_WARMUP !== "1" && !NO_LOGIN) {
  process.stderr.write("\n== warm-up (discarded) ==\n");
  for (let attempt = 1; attempt <= 2; attempt += 1) {
    const result = await timed(`warmup signRaw #${attempt}`, signRawOp);
    if (result.ok) {
      break;
    }
  }
}

// --- Phase 1: baseline, no AutoSigning ------------------------------------
// Skipped in GRANT_ONLY: without it no suppression claim is possible, which is
// the point -- that mode answers "is the grant answered", not "does it work".
const baseline = GRANT_ONLY
  ? { signRaw: emptyStats(), createTransaction: emptyStats() }
  : await runPhase("phase 1: baseline (no AutoSigning)");

// --- Phase 2: request the grant ------------------------------------------
process.stderr.write("\n== phase 2: request AutoSigning ==\n");
const resources: AllocatableResource[] = [
  { tag: "AutoSigning" },
  { tag: "BulletinAllowance" },
];
let outcomes: AllocationOutcome[] = [];
if (NO_LOGIN) {
  process.stderr.write("   SKIPPED (no session to allocate against)\n");
} else try {
  const allocation = await client.resourceAllocation.request({ resources });
  if (allocation.isOk()) {
    outcomes = allocation.value.outcomes;
    resources.forEach((resource, i) => {
      process.stderr.write(
        `   ${resource.tag} -> ${outcomes[i] ?? "no outcome"}\n`,
      );
    });
  } else {
    process.stderr.write(
      `   allocation failed: ${JSON.stringify(allocation.error)}\n`,
    );
  }
} catch (error) {
  process.stderr.write(
    `   allocation threw: ${explainProductError(error) ?? String(error)}\n`,
  );
}
const autoSigningGranted = outcomes[0] === "Allocated";
const bulletinAllowanceGranted = outcomes[1] === "Allocated";

// --- Phase 2b: Bulletin writes under the granted allowance ----------------
// What this measures is LATENCY of the Bulletin write path, which is the gate
// that no permission setting can move. Prompt count is recorded too, but it is
// predicted entirely by AutoSigning -- an allowance is a chain-side write
// quota, not a consent policy, so it was never going to suppress a review.
// Absolute measurement, not before/after: the allowance is already live on
// chain, so there is no meaningful "before" to compare against.
if (PREIMAGE) {
  process.stderr.write(
    "\n== phase 2b: Bulletin writes (real chain writes) ==\n",
  );
  if (!bulletinAllowanceGranted) {
    process.stderr.write(
      `   BulletinAllowance is ${outcomes[1] ?? "absent"}; submitting anyway so\n` +
        "   the unallocated behaviour is on record too.\n",
    );
  }
  if (CONCURRENCY > 0) {
    const size = PREIMAGE_SIZES[0] ?? 4096;
    process.stderr.write(
      `   Firing ${String(CONCURRENCY)} × ${size.toLocaleString()}B writes CONCURRENTLY.\n` +
        `   Compare against the sequential median: ~N× faster => the core pipelines;\n` +
        `   ~same total as N sequential writes => it serialises.\n`,
    );
    const t0 = performance.now();
    const settled = await Promise.all(
      Array.from({ length: CONCURRENCY }, (_, i) =>
        preimageSubmitOp(size, 1000 + i)
          .then((r) => ({ ok: r.ok, detail: r.detail }))
          .catch((e: unknown) => ({ ok: false, detail: String(e) })),
      ),
    );
    const wall = performance.now() - t0;
    const okN = settled.filter((r) => r.ok).length;
    process.stderr.write(
      `\n   === concurrency ===\n` +
        `   ${String(okN)}/${String(CONCURRENCY)} succeeded\n` +
        `   wall clock : ${fmt(wall)} for ${String(CONCURRENCY)} writes\n` +
        `   per write  : ${fmt(wall / Math.max(okN, 1))} amortised\n`,
    );
    for (const r of settled.filter((x) => !x.ok)) {
      process.stderr.write(`   failed: ${r.detail.slice(0, 100)}\n`);
    }
    process.stderr.write(
      `\n   Read against a single sequential write (~11-20s measured):\n` +
        `   amortised well BELOW that => pipelined; at or ABOVE => serialised.\n`,
    );
    product.dispose();
    host.dispose();
    process.exit(okN === CONCURRENCY ? 0 : 1);
  }

  const writes = emptyStats();
  const sweep: { size: number; ms: number; ok: boolean; detail: string }[] = [];
  process.stderr.write(
    `   SSO payload ceiling is ${SSO_PAYLOAD_CEILING.toLocaleString()} bytes;\n` +
      `   anything larger that SUCCEEDS did not traverse SSO.\n`,
  );
  for (const [i, size] of PREIMAGE_SIZES.entries()) {
    const label = `preimage.submit ${size.toLocaleString()}B${
      size > SSO_PAYLOAD_CEILING ? " (OVER ceiling)" : ""
    }`;
    const result = await timed(label, () => preimageSubmitOp(size, i + 1));
    record(writes, result);
    sweep.push({ size, ms: result.ms, ok: result.ok, detail: result.detail });
    if (result.ssoTimeout) {
      process.stderr.write("   -> wallet did not answer; stopping writes\n");
      break;
    }
  }

  // The decisive read: did anything above the ceiling go through?
  const over = sweep.filter((s) => s.size > SSO_PAYLOAD_CEILING);
  const overOk = over.filter((s) => s.ok);
  const tooBig = sweep.filter((s) => /message too big/i.test(s.detail));
  process.stderr.write(`\n   === path verdict ===\n`);
  for (const s of sweep) {
    process.stderr.write(
      `   ${String(s.size).padStart(8)}B  ${fmt(s.ms).padStart(8)}  ${s.ok ? "ok" : s.detail.slice(0, 70)}\n`,
    );
  }
  if (overOk.length > 0) {
    process.stderr.write(
      `\n   => LOCAL SIGNING. ${overOk.length} payload(s) above the SSO ceiling\n` +
        `      succeeded, so Bulletin writes are signed in-core with the\n` +
        `      allocated allowance key and never enter the SSO channel.\n` +
        `      The per-write prompt is the only obstacle; AutoSigning would fix it.\n`,
    );
  } else if (tooBig.length > 0) {
    process.stderr.write(
      `\n   => SSO-RELAYED. Payloads above the ceiling are rejected with\n` +
        `      "message too big", so writes traverse the statement store and\n` +
        `      bulk data cannot pass. This is an architectural limit.\n`,
    );
  } else if (over.length > 0) {
    process.stderr.write(
      `\n   => INCONCLUSIVE. Over-ceiling payloads failed, but not with\n` +
        `      "message too big" — see the error above before concluding.\n`,
    );
  }

  const writeMedian = median(writes.latencies);
  const promptFree = writes.titles.length === 0;
  const fastEnough = !Number.isNaN(writeMedian) && writeMedian <= 1000;
  process.stderr.write(
    `\n   prompts: ${String(writes.titles.length)} across ${String(writes.latencies.length)} write(s)` +
      ` [${summarizeTitles(writes.titles)}]\n` +
      `   latency: median ${writeMedian.toFixed(0)}ms, max ${Math.max(...writes.latencies, 0).toFixed(0)}ms\n` +
      `   errors:  ${writes.failures.length === 0 ? "none" : writes.failures.join("; ")}\n` +
      `   => ${
        writes.latencies.length === 0
          ? "NO WRITE SUCCEEDED - see the error above; latency/prompt counts " +
            "say nothing when nothing was written"
          : writes.ssoTimeouts > 0
          ? "BLOCKED UPSTREAM - wallet did not answer a Bulletin write"
          : promptFree && fastEnough
            ? "PROMPT-FREE and fast - the allowance carries Bulletin writes. " +
              "This is the storage-signer path."
            : promptFree
              ? `PROMPT-FREE but SLOW - ${writeMedian.toFixed(0)}ms per write`
              : // NOT "the allowance does not cover the review": an allowance is
                // a chain-side quota, prompt suppression is AutoSigning's job.
                // The two are orthogonal, so attribute the prompt correctly.
                `PROMPTS PER WRITE (expected: AutoSigning is ${outcomes[0] ?? "absent"}) ` +
                `and ${writeMedian.toFixed(0)}ms per write`
      }\n`,
  );
}

if (GRANT_ONLY) {
  // The whole point of this mode: report whether the wallet ANSWERED, and
  // resist drawing any conclusion about suppression, which was not tested.
  process.stderr.write("\n== grant-only result ==\n");
  if (NO_LOGIN) {
    process.stderr.write(
      "Allocation was never requested (PROBE_NO_LOGIN=1), so nothing is known\n" +
        "about AutoSigning from this run.\n",
    );
  } else if (outcomes.length === 0) {
    process.stderr.write(
      "The wallet did not answer the allocation request.\n" +
        "=> BLOCKED UPSTREAM. AutoSigning cannot be evaluated on this mobile\n" +
        "   build, so the host-cli signing path has no route forward yet.\n",
    );
  } else {
    process.stderr.write(
      `AutoSigning       -> ${outcomes[0] ?? "no outcome"}\n` +
        `BulletinAllowance -> ${outcomes[1] ?? "no outcome"}\n` +
        (autoSigningGranted
          ? "=> AutoSigning IS allocated. Worth a full run (drop\n" +
            "   PROBE_GRANT_ONLY) to test whether it suppresses prompts.\n"
          : `=> AutoSigning is ${outcomes[0] ?? "absent"}, so per-call prompts stay.\n` +
            "   The wallet answered, so this is a capability gap, not a dead\n" +
            "   channel." +
            (bulletinAllowanceGranted
              ? " Note BulletinAllowance WAS allocated -- the two are\n" +
                "   independent, so storage is worth measuring separately.\n"
              : "\n")),
    );
    // Only disclaim what actually went unmeasured. Phase 2b, when it runs,
    // establishes prompts AND latency for the Bulletin write path -- saying
    // otherwise here contradicts the numbers printed directly above.
    process.stderr.write(
      PREIMAGE
        ? "\nEstablished above for Bulletin writes only. NOT established:\n" +
            "generic signRaw / createTransaction behaviour (phases 1 and 3\n" +
            "were skipped).\n"
        : "\nNOT established by this mode: whether prompts are suppressed, or\n" +
            "how fast a signature is. All signing phases were skipped.\n",
    );
  }
  product.dispose();
  host.dispose();
  process.exit(autoSigningGranted ? 0 : 1);
}

// --- Phase 3: same ops, post-grant ---------------------------------------
const postGrant = await runPhase("phase 3: post-grant");

// --- Verdict --------------------------------------------------------------
// bulletin-deploy's chunk loop needs BOTH: no prompt, and a signature fast
// enough that ~50 of them are not the dominant cost of a deploy.
const LATENCY_BUDGET_MS = 1000;

process.stderr.write("\n== verdict ==\n");
process.stderr.write(
  `AutoSigning allocation: ${autoSigningGranted ? "Allocated" : (outcomes[0] ?? "not granted")}\n`,
);
process.stderr.write(
  `BulletinAllowance:      ${outcomes[1] ?? "not granted"}\n\n`,
);

let viable = autoSigningGranted;
for (const op of OPS) {
  const before = baseline[op.key];
  const after = postGrant[op.key];
  const medianMs = median(after.latencies);

  let verdict: string;
  if (before.ssoTimeouts > 0 || after.ssoTimeouts > 0) {
    // The wallet never answered, so nothing about AutoSigning was exercised.
    // This is a MOBILE-SIDE gap, not a host or core one -- a different upstream
    // ask than "the prompt is not suppressed", so it gets its own verdict.
    verdict =
      "BLOCKED UPSTREAM - the wallet never answered; " +
      "AutoSigning was never exercised";
    viable = false;
  } else if (before.titles.length === 0) {
    // Never reached the review surface, so a post-grant zero proves nothing.
    verdict = "INCONCLUSIVE - op never prompted even without AutoSigning";
    viable = false;
  } else if (after.titles.length > 0) {
    verdict = "NOT SUPPRESSED - still prompts per signature";
    viable = false;
  } else if (Number.isNaN(medianMs) || medianMs > LATENCY_BUDGET_MS) {
    verdict = `SUPPRESSED but SLOW - ${medianMs.toFixed(0)}ms > ${String(LATENCY_BUDGET_MS)}ms budget`;
    viable = false;
  } else {
    verdict = "SUPPRESSED and fast";
  }

  process.stderr.write(
    `${op.key}\n` +
      `   prompts: ${String(before.titles.length)} baseline -> ${String(after.titles.length)} post-grant\n` +
      `   baseline titles:  ${summarizeTitles(before.titles)}\n` +
      `   post-grant titles: ${summarizeTitles(after.titles)}\n` +
      `   latency: median ${medianMs.toFixed(0)}ms, max ${Math.max(...after.latencies, 0).toFixed(0)}ms\n` +
      `   errors:  ${after.failures.length === 0 ? "none" : after.failures.join("; ")}\n` +
      `   => ${verdict}\n`,
  );
}

if (timedOut) {
  process.stderr.write(
    "\nCAVEAT: at least one op timed out. A request abandoned mid-flight can\n" +
      "still prompt later, inside a subsequent op's counting window, so every\n" +
      "prompt count after the first timeout is suspect. Re-run before trusting\n" +
      "any number above.\n",
  );
}

process.stderr.write(
  `\n${
    viable
      ? "VIABLE: AutoSigning suppresses prompts within the latency budget. " +
        "A PolkadotSigner adapter over createTransaction is worth building."
      : "NOT VIABLE as measured. bulletin-deploy cannot drive its chunk loop " +
        "through this host until the failing line above changes."
  }\n`,
);

product.dispose();
host.dispose();
process.exit(viable ? 0 : 1);
