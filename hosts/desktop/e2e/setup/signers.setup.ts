import { test as setup } from '@playwright/test';

import { CHATPAIR_POOL_SIZE } from '../fixtures/chatPair';
import { envToNetwork } from '../helpers/environment';
import { errorMessage } from '../helpers/errors';
import { slotHasIdentity, slotPath, warmSlot } from '../helpers/signing-host';

/**
 * Creates and attests any signer identity the invoked projects need that the cached
 * base-path tree does not already hold, then exits.
 *
 * Starting the host is the only provisioning trigger, so warming means running
 * `--serve` against each slot and stopping once it reports ready. On a warm tree each
 * slot restores in seconds; on a cold one it pays the on-chain attestation that would
 * otherwise land inside a test's timeout.
 */

/**
 * Mirrors `playwright.config.ts`'s `E2E_WORKERS ?? (CI ? 4 : '50%')` and MUST be kept in
 * step with it: a slot this setup does not warm is not skipped, it is provisioned inside
 * the first test that claims it — paying a cold on-chain attestation against that test's
 * timeout. The `'50%'` case cannot be resolved here, so a local run wanting warmed
 * identities sets `E2E_WORKERS` explicitly; the fallback matches CI.
 */
const WORKERS = Number(process.env['E2E_WORKERS']) || 4;

/**
 * Projects whose npm script pins `--workers`, which this module cannot see: it reads
 * `E2E_WORKERS`, and the CLI flag does not set it. Warming `auth-w1` would attest an
 * identity no run ever claims, and every wasted attestation is an on-chain attestation
 * paid for nothing. A project pinned in `package.json` MUST get an entry here.
 */
const WORKERS_FOR: Record<string, number> = { auth: 1 };

/**
 * Only the projects actually invoked. Attesting a slot mints a durable on-chain identity
 * and spends one of its daily allowance slots, so warming all of them on
 * `npm run test:e2e:auth` would pay for identities the run never touches.
 *
 * `E2E_SIGNER_PROJECTS` is set by the per-project npm scripts. It MUST sit on the
 * `playwright` command, not at the head of the script: in POSIX shell `VAR=v a && b`
 * scopes `VAR` to `a` alone, and this setup runs inside the Playwright process.
 */
const ALL_PROJECTS = ['auth', 'authenticated', 'product-sdk', 'chat'];
const KNOWN_PROJECTS = new Set(ALL_PROJECTS);

const invoked = (process.env['E2E_SIGNER_PROJECTS'] ?? ALL_PROJECTS.join(','))
  .split(',')
  .map(name => name.trim())
  .filter(name => KNOWN_PROJECTS.has(name));

const SLOTS: string[] = [
  ...invoked.flatMap(project => Array.from({ length: WORKERS_FOR[project] ?? WORKERS }, (_, index) => slotPath(project, index))),
  // The chat-pair fixture cycles a bounded pool of identity pairs in its own namespace,
  // indexed `parallelIndex * CHATPAIR_POOL_SIZE + n` (`e2e/fixtures/chatPair.ts`), so the
  // flat count is (chat's worker count) * CHATPAIR_POOL_SIZE pairs — two slots each. It
  // MUST use the same pinned count as the `chat` slots above, or it warms pairs no worker
  // can address.
  //
  // The import direction matters: `chatPair.ts` declares the constant and this module
  // reads it. Importing this module from a test worker would execute the
  // `setup('warm signer slots', …)` call below, which Playwright rejects.
  ...(invoked.includes('chat')
    ? Array.from({ length: (WORKERS_FOR['chat'] ?? WORKERS) * CHATPAIR_POOL_SIZE }, (_, slot) => [
        slotPath('chatpair', slot, 'a'),
        slotPath('chatpair', slot, 'b'),
      ]).flat()
    : []),
];

const CONCURRENCY = Number(process.env['E2E_PROVISION_CONCURRENCY']) || 4;

/**
 * Run `tasks` with at most `limit` in flight, settling every one, and report each
 * outcome positionally.
 *
 * Settling every task is the whole point. This was `Promise.all`, which rejects on the
 * first failing task while the other workers keep running — so nothing ever awaited their
 * `finally` blocks and their `truapi-host` processes outlived the run holding flock on
 * their account stores. The immediate Playwright retry then died on every one of them
 * with `another truapi-host is using the account store … (Resource temporarily
 * unavailable (os error 11))`, turning one slot's chain-side flake into a whole suite.
 */
async function runWithConcurrency<T>(tasks: (() => Promise<T>)[], limit: number): Promise<PromiseSettledResult<T>[]> {
  const results: PromiseSettledResult<T>[] = [];
  let next = 0;
  const workers = Array.from({ length: Math.min(limit, tasks.length) }, async () => {
    while (next < tasks.length) {
      const index = next++;
      const task = tasks[index];
      if (!task) continue;
      try {
        results[index] = { status: 'fulfilled', value: await task() };
      } catch (reason) {
        results[index] = { status: 'rejected', reason };
      }
    }
  });
  // Safe now: a worker loop catches per task, so it cannot reject and strand its peers.
  await Promise.all(workers);

  return results;
}

/**
 * Warming is best-effort and this setup NEVER fails.
 *
 * A slot can fail to warm for reasons this harness does not control — the observed one is
 * the chain reporting `dotNS username did not appear on Asset Hub after attestation`. That
 * is one slot out of dozens, and failing here would block every test in the project on it.
 *
 * The cost is real and is accepted deliberately: an unwarmed slot is not skipped, it is
 * provisioned inside the first test that claims it, paying a cold on-chain attestation
 * (observed between 45s and past 120s) against that test's timeout. So that test will
 * probably fail — while the rest of the suite still runs. Failures are logged per slot and
 * attached as annotations so a degraded warm-up is visible in the report rather than
 * silent.
 */
setup('warm signer slots', async () => {
  const network = envToNetwork('nightly');
  if (SLOTS.length === 0) {
    console.info('[setup] no signer-backed project invoked; nothing to warm');

    return;
  }

  console.info(`[setup] warming ${SLOTS.length} signer slot(s) on ${network}, ${CONCURRENCY} at a time`);
  const results = await runWithConcurrency(
    SLOTS.map(basePath => async () => {
      // A slot whose identity cannot be restored will attest a fresh one. Report it before
      // paying for it — on a tree that should be warm this most often means the cache
      // restore was partial.
      if (!(await slotHasIdentity(basePath, network))) {
        console.info(`[setup] ${basePath}: no restorable identity, will provision`);
      }
      await warmSlot({ basePath, network });
    }),
    CONCURRENCY,
  );

  const failures = SLOTS.map((basePath, index) => ({ basePath, result: results[index] })).filter(
    entry => entry.result?.status === 'rejected',
  );

  if (failures.length === 0) {
    console.info(`[setup] warmed ${SLOTS.length}/${SLOTS.length} signer slot(s)`);

    return;
  }

  for (const { basePath, result } of failures) {
    const reason = result?.status === 'rejected' ? errorMessage(result.reason) : 'unknown';
    console.error(`[setup] ${basePath}: FAILED to warm — ${reason}`);
    setup.info().annotations.push({ type: 'signer-slot-not-warmed', description: `${basePath}: ${reason}` });
  }

  // Every slot failing is a different problem from one flaking — usually the binary or the
  // network, not the chain — and it would otherwise read as N indistinguishable test
  // failures later in the run.
  const scope = failures.length === SLOTS.length ? 'EVERY slot failed to warm' : `${failures.length} slot(s) failed to warm`;
  console.error(
    `[setup] ${scope} (${SLOTS.length - failures.length}/${SLOTS.length} warmed). Not failing the setup: each ` +
      `unwarmed slot will provision inside the first test that claims it, which may exceed that test's timeout.`,
  );
});
