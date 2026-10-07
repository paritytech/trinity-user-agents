// Statement proofs signed with another product's account, counting the
// confirmations a signing-host CLI raises for them.
//
// A game signals with one statement per move. When it plays as another
// product's account under a `context` grant, every proof needs the user to
// approve that account, and asking once per statement puts a prompt in front of
// every move. The host asks once per account for the life of the execution.
//
// The CLI appends one line per consulted confirmation to
// `TRUAPI_APPROVALS_LOG`, and this counts the `sign statement proof` lines.
// `scripts/cross-product-statement-proof-e2e.sh` runs it as `dim2next.paseo`,
// which `dim2.paseo`'s local product config grants `context`. Neither label
// is one the host trusts outright, which would skip the confirmation.

import { existsSync, readFileSync } from "node:fs";
import { actionLines, approvalLines } from "./auto-signing-e2e.ts";

const OWNER = "dim2.paseo";
const CALLER = "dim2next.paseo";
const PROOFS_PER_ACCOUNT = 5;
const ACTION = "sign statement proof";

function stringify(value: unknown): string {
  try {
    return JSON.stringify(value, (_, inner) =>
      typeof inner === "bigint" ? inner.toString() : inner,
    );
  } catch {
    return String(value);
  }
}

function ownerAccount(index: number) {
  return {
    dotNsIdentifier: OWNER,
    derivationIndex: { tag: "Index" as const, value: index },
  };
}

/// A statement a day out, so it is unexpired wherever it is checked.
function statement() {
  const expiry = BigInt(Math.floor(Date.now() / 1000) + 86400) << 32n;
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  return { expiry, topics: [`0x${bytes.toHex()}` as `0x${string}`] };
}

async function createProof(index: number): Promise<void> {
  const result = await truapi.statementStore.createProof({
    productAccountId: ownerAccount(index),
    statement: statement(),
  });
  if (!result.isOk()) {
    throw new Error(
      `${CALLER} could not sign a statement as ${OWNER}/${index}: ${stringify(result.error)}`,
    );
  }
}

const approvalsLog = process.env.TRUAPI_APPROVALS_LOG;
if (!approvalsLog) {
  throw new Error("TRUAPI_APPROVALS_LOG not set; cannot count confirmations");
}
if (host.productId !== CALLER) {
  throw new Error(
    `expects --product-id ${CALLER}, host serves ${host.productId}`,
  );
}

function statementPrompts(): number {
  const text = existsSync(approvalsLog)
    ? readFileSync(approvalsLog, "utf8")
    : "";
  return actionLines(approvalLines(text), ACTION).length;
}

/// Run `proofs`, then require the prompt count to have grown by `expected`.
async function expectPrompts(
  label: string,
  expected: number,
  proofs: () => Promise<unknown>,
): Promise<void> {
  const before = statementPrompts();
  await proofs();
  const prompts = statementPrompts() - before;
  console.log(`${label}: ${prompts} prompt(s), expected ${expected}`);
  if (prompts !== expected) {
    throw new Error(
      `${label}: the user was asked ${prompts} times, expected ${expected}`,
    );
  }
}

await expectPrompts(
  `${PROOFS_PER_ACCOUNT} proofs in a row as ${OWNER}/0`,
  1,
  async () => {
    for (let move = 0; move < PROOFS_PER_ACCOUNT; move++) {
      await createProof(0);
    }
  },
);

await expectPrompts(
  `${PROOFS_PER_ACCOUNT} proofs at once as ${OWNER}/1`,
  1,
  () =>
    Promise.all(
      Array.from({ length: PROOFS_PER_ACCOUNT }, () => createProof(1)),
    ),
);

await expectPrompts(`one more proof as ${OWNER}/0`, 0, () => createProof(0));

console.log("one confirmation per cross-product account");
