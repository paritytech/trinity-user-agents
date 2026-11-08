import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { TabSession } from "../../src/app/tab-session.js";
import { tabWalletKey } from "../../src/platform/network-choice.js";
import {
  testUserScope,
  type TestUser,
} from "../../fixtures/invitations/test-users.js";

/** What the invitation demo's tab session needs from the page and the demo. */
export interface DemoTabSessionDeps {
  network: { id: string };
  user: TestUser;
  /** Act as the test user `id` in this tab, or leave the demo with null. Reloads the tab. */
  useDemo(id: string | null): void;
  /** Sign the test user in on `runtime` and start its chat. */
  start(runtime: WorkerPairingHostRuntime): Promise<void>;
  /** Run `task` with the core, in order after every session change queued before it. */
  serial(
    task: (runtime: WorkerPairingHostRuntime) => Promise<void>,
  ): Promise<void>;
  /** Hand a session with no wallet keys, under `scope`, to what belongs to it, or end it with null. */
  syncProductsOnly(scope: string | null): void;
}

/**
 * DEV ONLY. The tab as the invitation demo's test user: signed in by the core
 * with the user's public test phrase, its session reaching only its product
 * workers and their chat. It opens no product the user did not choose, and
 * leaving it for a wallet reloads the tab signed in to that wallet.
 */
export function demoTabSession(deps: DemoTabSessionDeps): TabSession {
  const { user } = deps;
  return {
    activeId: () => "invitation-demo",
    signIn: (wallet) => {
      sessionStorage.setItem(tabWalletKey(deps.network.id), wallet.id);
      deps.useDemo(null);
    },
    signOut: () => deps.useDemo(null),
    // The name is made up; any username the public phrase's account holds
    // is not the demo user's.
    name: () => `${user.name} (invitation demo)`,
    evidence: () => ({ id: user.id, who: `test user ${user.name}` }),
    sync: (account) =>
      deps.syncProductsOnly(account === null ? null : testUserScope(user)),
    resume: () => deps.serial((runtime) => deps.start(runtime)),
    autoOpens: false,
    emptyStageNote: `Search for a product to try it as ${user.name}.`,
    // Chat shows the demo's chat workers once the test user signs in.
    noChat: {
      title: "Chat starts with the invitation demo",
      body: "The chat products run once the test user is signed in.",
    },
  };
}
