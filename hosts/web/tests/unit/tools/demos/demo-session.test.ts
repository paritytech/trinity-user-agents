import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { ConnectedAccount } from "../../../../src/app/tab-session.js";
import { tabWalletKey } from "../../../../src/platform/network-choice.js";
import type { Wallet } from "../../../../src/platform/wallets.js";
import { TEST_USERS } from "../../../../fixtures/invitations/test-users.js";
import { demoTabSession } from "../../../../tools/demos/demo-session.js";
import { MemoryStorage } from "../../../support/test-support.js";

/**
 * The tab as the invitation demo's test user. Choosing a wallet or signing
 * out leaves the demo by a reload, never by signing the wallet in beside the
 * test user; its session reaches only its product workers; and it reopens no
 * product the user did not choose.
 */

const saved = (globalThis as { sessionStorage?: unknown }).sessionStorage;
beforeAll(() => {
  Object.assign(globalThis, { sessionStorage: new MemoryStorage() });
});
afterAll(() => {
  Object.assign(globalThis, { sessionStorage: saved });
});

function demo() {
  const user = TEST_USERS[0]!;
  const left: (string | null)[] = [];
  const synced: (string | null)[] = [];
  const started: WorkerPairingHostRuntime[] = [];
  const runtime = {} as WorkerPairingHostRuntime;
  const session = demoTabSession({
    network: { id: "paseo" },
    user,
    useDemo: (id) => left.push(id),
    start: async (current) => {
      started.push(current);
    },
    serial: (task) => task(runtime),
    syncProductsOnly: (scope) => synced.push(scope),
  });
  return { user, session, left, synced, started, runtime };
}

describe("the invitation demo's tab session", () => {
  test("leaves the demo for a chosen wallet, which the reload signs in", () => {
    const { session, left } = demo();
    session.signIn({ id: "wallet-1" } as Wallet);
    expect({
      tabWallet: sessionStorage.getItem(tabWalletKey("paseo")),
      left,
    }).toEqual({ tabWallet: "wallet-1", left: [null] });
    session.signOut();
    expect(left).toEqual([null, null]);
  });

  test("hands its session only to its product workers, under the test user's scope, and ends it on sign-out", () => {
    const { user, session, synced } = demo();
    session.sync({} as ConnectedAccount);
    session.sync(null);
    expect(synced).toEqual([`test-user:${user.id}`, null]);
  });

  test("signs the test user in once the core runs, and opens no product by itself", async () => {
    const { user, session, started, runtime } = demo();
    await session.resume();
    expect({
      started,
      autoOpens: session.autoOpens,
      active: session.activeId(),
      name: session.name({} as ConnectedAccount),
      evidence: session.evidence(),
    }).toEqual({
      started: [runtime],
      autoOpens: false,
      active: "invitation-demo",
      name: `${user.name} (invitation demo)`,
      evidence: { id: user.id, who: `test user ${user.name}` },
    });
  });
});
