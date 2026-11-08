import type { AuthState } from "@parity/truapi-host";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { ProductNavigation } from "./navigation.js";
import type { ChatV2Host } from "../chat/draft/host.js";
import type { ChatInbox } from "../chat/inbox.js";
import { productChatKey, type ProductChat } from "../chat/product-chat.js";
import type { HostNames } from "../contacts/host-names.js";
import { chainDirectory } from "../platform/chain-directory.js";
import { sessionChatKeys } from "../invitations/chat-keys.js";
import {
  walletRecordKey,
  type WalletContacts,
} from "../contacts/wallet-contacts.js";
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

type Runtime = WorkerPairingHostRuntime;

/** What the session needs from the page around it. */
export interface WalletSessionDeps {
  network: NetworkConfig;
  storage: HostStorage;
  walletInfo: WalletPublicInfoStore;
  navigation: ProductNavigation;
  serial(task: (runtime: Runtime) => Promise<void>): Promise<void>;
  activity: ActivitySink;
  /**
   * Run once a wallet's core session is active, before any product reopens:
   * the wallet's backup restores here (`../backup/host-backup.ts`).
   */
  afterSignIn?(wallet: Wallet): Promise<void>;
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
      await this.deps.afterSignIn?.(wallet);
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
  hostNames: HostNames;
  walletContacts: WalletContacts;
  /** Whether the wallet's saved contacts stay out of the picker for this page. */
  hidesWalletContacts(): boolean;
  chatV2: ChatV2Host;
  chatInbox: ChatInbox;
}

/**
 * Hand a session, or none, to everything that belongs to it. A new scope
 * ends the draft bridge's rooms of the one before.
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
      hostNames,
      walletContacts,
      chatV2,
      chatInbox,
    } = this.members;
    if (session === null) {
      pocketFaces.setSession(null);
      productWorkers.setSession(null);
      productChat.setScope(null);
      notifications?.setScope(this.members.signedOutNotifications);
      hostNames.setSession(null);
      walletContacts.setSession(null);
      if (this.chatV2Scope !== null) chatV2.reset();
      this.chatV2Scope = null;
      chatInbox.setSession(null);
      return;
    }
    const { wallet, access } = session;
    const { identityAccountId, chatPublicKey } = session.account;
    const recordKey = walletRecordKey(network.id, wallet.id);
    hostNames.setSession({
      scope: `${network.id}:${wallet.id}`,
      record: () => localStorage.getItem(recordKey),
      directory: () => chainDirectory(access.chain, access.genesis),
    });
    // Contacts are offered only for keys the core confirms, as for any of
    // the wallet's requests: never for the public development phrase.
    walletContacts.setSession({
      key: recordKey,
      scope: `${recordKey}:${identityAccountId ?? ""}:${chatPublicKey ?? ""}`,
      offered: () =>
        !this.members.hidesWalletContacts() &&
        typeof sessionChatKeys({
          entropy: wallet.entropy,
          networkSuffix: access.networkSuffix,
          identityAccountId: identityAccountId ?? undefined,
          chatPublicKey: chatPublicKey ?? undefined,
        }) !== "string",
    });
    const scope = productChatKey(network.id, wallet.id);
    if (scope !== this.chatV2Scope) chatV2.reset();
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
      network: network.id,
      identity: `${identityAccountId ?? ""}:${chatPublicKey ?? ""}`,
    });
  }

  /**
   * Hand a session with no wallet keys, such as a test user of the
   * invitation demo, to what belongs to it: its product workers and their
   * chat providers. Pocket, the wallet's saved contacts and TrUAPI product
   * rooms stay off, and the Inbox reads the scope it reads while signed out. Null ends it,
   * as {@link sync} with none does.
   */
  syncProductsOnly(walletId: string | null): void {
    if (walletId === null) return this.sync(null);
    const {
      network,
      pocketFaces,
      productWorkers,
      productChat,
      notifications,
      hostNames,
      walletContacts,
      chatV2,
      chatInbox,
    } = this.members;
    pocketFaces.setSession(null);
    productChat.setScope(null);
    notifications?.setScope(this.members.signedOutNotifications);
    hostNames.setSession(null);
    walletContacts.setSession(null);
    const scope = `${network.id}:${walletId}:products-only`;
    if (scope !== this.chatV2Scope) chatV2.reset();
    this.chatV2Scope = scope;
    productWorkers.setSession({ walletId, network });
    chatInbox.setSession({
      walletId,
      network: network.id,
      identity: "products-only",
    });
  }
}
