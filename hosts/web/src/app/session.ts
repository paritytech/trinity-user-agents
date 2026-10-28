import type { AuthState } from "@parity/truapi-host";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { ProductNavigation } from "./navigation.js";
import type { ChatV2Host } from "../chat/draft/host.js";
import type { ChatInbox } from "../chat/inbox.js";
import { sessionChatKeys, type ChatSession } from "../chat/nova-session.js";
import { productChatKey, type ProductChat } from "../chat/product-chat.js";
import type { HostNames } from "../contacts/host-names.js";
import { chainDirectory } from "../chat/nova/directory.js";
import { chatRecordKey } from "../chat/nova/store.js";
import {
  notificationsKey,
  type NotificationStore,
} from "../notifications/notifications.js";
import type { PocketFaces } from "../pocket/faces.js";
import { pinnedCards } from "../pocket/pocket.js";
import type { ProductWorkers } from "../products/product-workers.js";
import type { ActivitySink } from "../platform/activity.js";
import type { Network } from "../platform/network.js";
import type { NetworkConfig } from "../platform/network-config.js";
import { tabWalletKey } from "../platform/network-choice.js";
import type { HostStorage } from "../platform/storage.js";
import type { Wallet } from "../platform/wallets.js";
import type { WalletPublicInfoStore } from "./wallet-label.js";
import {
  NOVA_WORKER_PRODUCT,
  type NovaWorkerMode,
} from "../../tools/demos/worker-mode.js";

type Runtime = WorkerPairingHostRuntime;

/** What the session needs from the page around it. */
export interface WalletSessionDeps {
  network: NetworkConfig;
  storage: HostStorage;
  walletInfo: WalletPublicInfoStore;
  navigation: ProductNavigation;
  serial(task: (runtime: Runtime) => Promise<void>): Promise<void>;
  activity: ActivitySink;
}

/**
 * The wallet this tab is signed in with, and how it changes. Every change
 * runs with the products closed and reopens the one in front afterwards
 * ({@link ProductNavigation.aroundSessionChange}), so no product outlives the
 * session it was opened under.
 */
export class WalletSession {
  private wallet: Wallet | null = null;

  constructor(private readonly deps: WalletSessionDeps) {}

  /** The signed-in wallet, or null. */
  get active(): Wallet | null {
    return this.wallet;
  }

  /** Run a session change with the product closed, in order after what was queued before. */
  change(change: (runtime: Runtime) => Promise<void>): Promise<void> {
    return this.deps.serial((runtime) =>
      this.deps.navigation.aroundSessionChange(runtime, () => change(runtime)),
    );
  }

  signIn(wallet: Wallet): Promise<void> {
    const { network, storage, walletInfo, activity } = this.deps;
    return this.change(async (runtime) => {
      if (this.wallet) await this.signOutOf(runtime);
      storage.useWallet(wallet.id, network.id);
      this.wallet = wallet;
      try {
        await runtime.activateLocalSession(
          wallet.entropy,
          walletInfo.get(wallet.id, network.id)?.sessionUsername,
        );
      } catch (error) {
        storage.useWallet(null, network.id);
        this.wallet = null;
        throw new Error(
          `Could not sign in with ${wallet.name}: ${String(error)}`,
        );
      }
      sessionStorage.setItem(tabWalletKey(network.id), wallet.id);
      activity({
        category: "session",
        action: "Signed in",
        subject: wallet.name,
        details: [{ label: "Network", value: network.displayName }],
      });
    });
  }

  signOut(): Promise<void> {
    return this.change((runtime) => this.signOutOf(runtime));
  }

  private async signOutOf(runtime: Runtime): Promise<void> {
    const { network, storage, activity } = this.deps;
    await runtime.disconnectSession();
    storage.useWallet(null, network.id);
    sessionStorage.removeItem(tabWalletKey(network.id));
    activity({
      category: "session",
      action: "Signed out",
      subject: this.wallet?.name ?? "wallet",
    });
    this.wallet = null;
  }
}

/** A session the core reports as connected, with the wallet and chains it runs on. */
export interface SignedIn {
  wallet: Wallet;
  access: Pick<Network, "chain" | "genesis" | "networkSuffix">;
  account: Extract<AuthState, { tag: "Connected" }>["value"];
}

/** Everything whose state belongs to one signed-in session. */
export interface SessionMembers {
  network: NetworkConfig;
  pocketFaces: PocketFaces;
  productWorkers: ProductWorkers;
  productChat: ProductChat;
  notifications: NotificationStore | null;
  /** The scope the Inbox reads while signed out: a test user's, or none. */
  signedOutNotifications: string | null;
  /** The dev server's local chat product demo, when it is offered. */
  localChatDemo: { stop(): void } | null;
  /** Where the dev server runs the wallet's Nova chat, when it offers the choice. */
  novaWorkerMode: Pick<NovaWorkerMode, "setSession"> | null;
  hostNames: HostNames;
  chatV2: ChatV2Host;
  chatInbox: ChatInbox;
  /** The Developer menu's engine switch, once it is drawn. */
  engineSwitch(): { render(): void } | null;
  proofs: NonNullable<ChatSession["proofs"]>;
}

/**
 * Hand a session, or none, to everything that belongs to it, in an order
 * that matters: the Nova engine's place is chosen before the workers and the
 * inbox start, and a new scope ends the draft bridge's rooms and the local
 * demo of the one before.
 */
export class SessionMembership {
  /** The wallet and network the v2 declarations belong to. */
  private chatV2Scope: string | null = null;

  constructor(private readonly members: SessionMembers) {}

  sync(session: SignedIn | null): void {
    const {
      network,
      pocketFaces,
      productWorkers,
      productChat,
      notifications,
      localChatDemo,
      novaWorkerMode,
      hostNames,
      chatV2,
      chatInbox,
    } = this.members;
    if (session === null) {
      pocketFaces.setSession(null);
      productWorkers.setSession(null);
      productChat.setScope(null);
      notifications?.setScope(this.members.signedOutNotifications);
      localChatDemo?.stop();
      novaWorkerMode?.setSession(null);
      hostNames.setSession(null);
      if (this.chatV2Scope !== null) chatV2.reset();
      this.chatV2Scope = null;
      chatInbox.setSession(null);
      this.members.engineSwitch()?.render();
      return;
    }
    const { wallet, access } = session;
    const { identityAccountId, chatPublicKey, fullUsername, liteUsername } =
      session.account;
    hostNames.setSession({
      scope: `${network.id}:${wallet.id}`,
      record: () => localStorage.getItem(chatRecordKey(network.id, wallet.id)),
      directory: () => chainDirectory(access.chain, access.genesis),
    });
    // First, so the workers and the inbox below see this session's engine choice.
    const restartNova = novaWorkerMode?.setSession({
      network: network.id,
      walletId: wallet.id,
      identity: `${identityAccountId ?? ""}:${chatPublicKey ?? ""}`,
      keys: () =>
        sessionChatKeys({
          entropy: wallet.entropy,
          networkSuffix: access.networkSuffix,
          identityAccountId: identityAccountId ?? undefined,
          chatPublicKey: chatPublicKey ?? undefined,
        }),
    });
    const scope = productChatKey(network.id, wallet.id);
    if (scope !== this.chatV2Scope) {
      // The demo belongs to the session it was started in.
      localChatDemo?.stop();
      chatV2.reset();
    }
    this.chatV2Scope = scope;
    productChat.setScope(scope);
    notifications?.setScope(notificationsKey(network.id, wallet.id));
    productWorkers.setSession({ walletId: wallet.id, network });
    pocketFaces.setSession({
      walletId: wallet.id,
      networkId: network.id,
      cards: pinnedCards(network.networkSuffix),
    });
    chatInbox.setSession({
      walletId: wallet.id,
      entropy: wallet.entropy,
      network: network.id,
      networkSuffix: access.networkSuffix,
      identityAccountId: identityAccountId ?? undefined,
      chatPublicKey: chatPublicKey ?? undefined,
      username: fullUsername ?? liteUsername ?? undefined,
      chain: access.chain,
      genesis: access.genesis,
      identityBackend: network.identityBackend,
      proofs: this.members.proofs,
    });
    if (restartNova === true) {
      // Its frame holds the link of the identity that ended; a new frame asks again.
      chatV2.forget(NOVA_WORKER_PRODUCT);
      void productWorkers.restart(NOVA_WORKER_PRODUCT);
    }
    this.members.engineSwitch()?.render();
  }
}
