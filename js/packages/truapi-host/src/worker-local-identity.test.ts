import { afterEach, expect, spyOn, test } from "bun:test";
import { resolveLocalIdentity } from "./worker-local-identity.js";
import type { WorkerSigningHostRuntime } from "./wasm-module.js";
import type { LocalIdentityProgress } from "./worker-protocol.js";

const account = `0x${"11".repeat(32)}`;
const registration = {
  baseUsername: "lateconfirmation",
  identityBackendBaseUrl: "https://identity.invalid/api/v1",
};
const spies: { mockRestore(): void }[] = [];
afterEach(() => {
  for (const spy of spies.splice(0)) spy.mockRestore();
});

function setup(
  refresh: () => Promise<{ identityAccountId: string; liteUsername?: string }>,
) {
  let submissions = 0;
  const realTimeout = globalThis.setTimeout;
  spies.push(
    spyOn(globalThis, "setTimeout").mockImplementation(((
      callback: () => void,
      delay?: number,
    ) =>
      realTimeout(callback, delay === 4_000 ? 0 : delay)) as typeof setTimeout),
  );
  spies.push(
    spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const path = new URL(String(input)).pathname;
      if (path.endsWith("/attester"))
        return Response.json({ attester: account });
      if (path.endsWith("/auth/challenges"))
        return Response.json({ challenge: "AA==" });
      if (path.endsWith("/auth/token"))
        return Response.json({ token: "test-proof-token" });
      if (path.endsWith("/usernames")) {
        submissions++;
        return new Response("", { status: 202 });
      }
      throw new Error(`Unexpected request: ${path}`);
    }),
  );
  const runtime = {
    localIdentityContext: () => ({
      activationId: 1,
      identityAccountId: account,
    }),
    refreshLocalIdentity: refresh,
    localIdentityAuthProof: () => new Uint8Array(64),
    localLiteRegistrationBody: async () => "{}",
  } as unknown as WorkerSigningHostRuntime;
  return { runtime, submissions: () => submissions };
}

test("an accepted claim confirms after the old polling cutoff without resubmission", async () => {
  let reads = 0;
  const confirmed = {
    identityAccountId: account,
    liteUsername: "lateconfirmation.paseo",
  };
  const { runtime, submissions } = setup(async () => {
    reads++;
    if (reads === 32) throw new Error("temporary chain disconnect");
    return reads > 40 ? confirmed : { identityAccountId: account };
  });
  await expect(
    resolveLocalIdentity(runtime, new AbortController().signal, registration),
  ).resolves.toEqual(confirmed);
  expect(submissions()).toBe(1);
});

test("accepted registration reports read retries and recovers without another submission", async () => {
  const progress: LocalIdentityProgress[] = [];
  const confirmed = {
    identityAccountId: account,
    liteUsername: "lateconfirmation.paseo",
  };
  let reads = 0;
  const { runtime, submissions } = setup(async () => {
    reads++;
    if (reads === 2) throw new Error("temporary chain disconnect");
    return reads === 4 ? confirmed : { identityAccountId: account };
  });
  await expect(
    resolveLocalIdentity(
      runtime,
      new AbortController().signal,
      registration,
      (event) => {
        progress.push(event);
        throw new Error("observer failed");
      },
    ),
  ).resolves.toEqual(confirmed);
  expect(progress).toEqual([
    { stage: "checking" },
    { stage: "authenticating" },
    { stage: "submitting" },
    { stage: "confirming" },
    { stage: "retrying", error: "temporary chain disconnect" },
    { stage: "confirming" },
    { stage: "confirming" },
  ]);
  expect(submissions()).toBe(1);
});

test("disposing the wallet stops confirmation monitoring without resubmission", async () => {
  const controller = new AbortController();
  let reads = 0;
  const { runtime, submissions } = setup(async () => {
    if (++reads === 4) controller.abort(new Error("wallet disposed"));
    return { identityAccountId: account };
  });
  await expect(
    resolveLocalIdentity(runtime, controller.signal, registration),
  ).rejects.toThrow("wallet disposed");
  expect(submissions()).toBe(1);
});

test("confirmation from a different identity is rejected", async () => {
  let reads = 0;
  const { runtime } = setup(async () =>
    ++reads === 1
      ? { identityAccountId: account }
      : {
          identityAccountId: `0x${"22".repeat(32)}`,
          liteUsername: "wrong.paseo",
        },
  );
  await expect(
    resolveLocalIdentity(runtime, new AbortController().signal, registration),
  ).rejects.toThrow("verified identity does not match");
});
