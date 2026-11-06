import type { ContactHandle } from "@parity/truapi";
import type { HandleNames } from "../contacts/contacts.js";
import type {
  ChatV2ProviderOptions,
  HandleContacts,
} from "../chat/draft/provider.js";
import type { DirectoryContacts } from "./directory-contacts.js";
import {
  InvitationService,
  productPort,
  type AcceptPrompt,
  type SendPrompt,
} from "./invitation-service.js";
import type {
  ConversationAllowance,
  HostDelivery,
  IdentityAuth,
  Product,
} from "./ports.js";
import type { InvitationObserver } from "./observer.js";
import type { ReplySuite } from "./binding.js";
import { cryptoRandom } from "./sealing.js";
import type { NotificationStore } from "../notifications/notifications.js";
import type { IdentityProblem } from "./published-keys.js";
import type { HostNotification } from "../notifications/notifications.js";
import type { SentRequest } from "../notifications/screen.js";

/**
 * Contact Chat's host side for one signed-in user: its request service, over
 * the user's wallet keys and the people a {@link DirectoryContacts} knows. A
 * wallet signed in from the Account picker runs it (`./wallet-chat.ts`), and
 * so does the invitation demo's test user on the dev server.
 *
 * The request service starts only once `verify` found the wallet keys fit to
 * prove the user; until then Contact Chat's port is held, and closed if they
 * are not. Requests and replies go through the delivery the caller hands it:
 * the Statement Store's, or the invitation demo's simulated one.
 */

export interface ContactChatHostOptions {
  product: Product;
  /** Where this user's sent and received requests of Contact Chat are kept. */
  keys: { sent: string; received: string };
  auth: IdentityAuth;
  /** How requests and replies travel. */
  delivery: HostDelivery;
  /** The reply-key suite of the next new request. Absent: the service's default. */
  replySuite?(): ReplySuite;
  /**
   * How Contact Chat's conversation accounts get their Statement Store
   * allowance. Absent: they get none, and Contact Chat cannot post text.
   */
  accounts?: ConversationAllowance;
  contacts: DirectoryContacts;
  handles: HandleNames;
  /** Calls back whenever a name the host draws may have changed, besides the handles. */
  onNamesChange?(listener: () => void): () => void;
  notifications: NotificationStore;
  /** Why the keys may not prove this user, or null when they may. */
  verify(): Promise<IdentityProblem | null>;
  confirmSend(prompt: SendPrompt): Promise<boolean>;
  /** Ask before an Accept reaches Contact Chat, with what it costs. Absent: the Accept is the answer. */
  confirmAccept?(prompt: AcceptPrompt): Promise<boolean>;
  showChat(): void;
  /** Run `task` after `delayMs`; answers a cancel. Defaults to `setTimeout`. */
  schedule?(task: () => void, delayMs: number): () => void;
  /** Calls back when the people of {@link ContactChatHost.people} changed. */
  onSenders?(): void;
  log(line: string): void;
  /** Told the request service's facts, for the inspector's Statements view. */
  observe?: InvitationObserver;
}

export interface ContactChatHost {
  product: Product;
  /** What the host lends Contact Chat's chat provider: names by handle, and starts by handle. */
  chatOptions: ChatV2ProviderOptions;
  /** Serve Contact Chat's request service port, in place of any before it. */
  attachChat(port: MessagePort): void;
  detachChat(): void;
  /** Whether `id` is a request this user received for Contact Chat. */
  owns(id: string): boolean;
  accept(item: HostNotification): Promise<void>;
  open(item: HostNotification): void;
  /**
   * The identity accounts of everyone this user's requests are with: who
   * sent them one, whom they sent one, and whom they are starting a chat
   * with. The host names them by handle; naming is not permission to pay.
   */
  people(): string[];
  /** This user's pending sent requests, for the host's Inbox. */
  sent(): SentRequest[];
  /** Cancel a pending sent request in the host. False when it is not one. */
  cancel(id: string): boolean;
  onSentChange(listener: () => void): () => void;
  /** Check the keys, then start the request service. */
  start(): Promise<void>;
  /** Stop everything this started; nothing is removed. */
  stop(): void;
}

/** How long Contact Chat waits to check the user's identity again after the chain could not be read. */
export const VERIFY_RETRY_MS = 30_000;

/** How long a start waits for the core to mint the handle of someone chosen by username. */
const NAMING_TIMEOUT_MS = 10_000;

export function startContactChatHost(
  options: ContactChatHostOptions,
): ContactChatHost {
  const now = () => Date.now();
  const schedule =
    options.schedule ??
    ((task: () => void, delayMs: number) => {
      const timer = setTimeout(task, Math.min(delayMs, 2 ** 31 - 1));
      return () => clearTimeout(timer);
    });
  const { contacts, handles } = options;
  const service = new InvitationService({
    product: options.product,
    auth: options.auth,
    contacts,
    notifications: options.notifications,
    storage: localStorage,
    keys: options.keys,
    delivery: options.delivery,
    now,
    schedule,
    random: cryptoRandom,
    ...(options.replySuite === undefined
      ? {}
      : { replySuite: options.replySuite }),
    confirmSend: options.confirmSend,
    ...(options.confirmAccept === undefined
      ? {}
      : { confirmAccept: options.confirmAccept }),
    showProduct: () => options.showChat(),
    ...(options.accounts === undefined ? {} : { accounts: options.accounts }),
    log: options.log,
    ...(options.observe === undefined ? {} : { observe: options.observe }),
  });
  /** Contact Chat's request port, held until the keys are checked. */
  let pending: MessagePort | null = null;
  let state: "checking" | "running" | "off" | "stopped" = "checking";
  /** Why the last check did not let the request service run, or null. */
  let problem: IdentityProblem | null = null;
  let cancelRecheck = () => {};
  /** Who draws whether a chat can be started: told whenever that changes. */
  const startListeners = new Set<() => void>();
  const startChanged = () => {
    for (const listener of [...startListeners]) listener();
  };
  /** People the user chose in New chat whose handle the host may not have yet. */
  const starting = new Set<string>();
  const people = () => [
    ...new Set([
      ...service.received.list().map((item) => item.context.from),
      ...service.outbox.list().map((item) => item.context.to),
      ...starting,
    ]),
  ];
  let peopleCount = 0;
  const sendersChanged = () => {
    const count = people().length;
    if (count === peopleCount) return;
    peopleCount = count;
    options.onSenders?.();
  };
  /** The handle of `account` once the host's index names them, or null after `timeoutMs`. */
  const handleOnceNamed = (account: string, timeoutMs: number) =>
    new Promise<ContactHandle | null>((resolve) => {
      const done = (handle: ContactHandle | null) => {
        clearTimeout(timer);
        stop();
        resolve(handle);
      };
      const check = () => {
        const handle = handleContacts.handleFor(account);
        if (handle !== null) done(handle);
      };
      const stop = handles.onChange(check);
      const timer = setTimeout(() => done(null), timeoutMs);
      check();
    });
  const stopNotes = options.notifications.onChange(sendersChanged);
  const stopSent = service.onOutgoing(sendersChanged);

  // Naming needs no chat key: the host draws whom a handle names even when
  // it cannot reach them.
  const handleContacts: HandleContacts = {
    resolve: (handle: ContactHandle) => {
      const account = handles.accountFor(handle.bytes);
      const person = account === null ? undefined : contacts.nameOf(account);
      return account === null || person === undefined
        ? null
        : { account, name: person.name, seed: person.handle };
    },
    handleFor: (account) => contacts.handleForIdentity(account),
  };
  const onStorage = (event: StorageEvent) => {
    if (
      state === "running" &&
      (event.key === null || Object.values(options.keys).includes(event.key))
    )
      service.reload();
  };
  window.addEventListener("storage", onStorage);

  const stop = () => {
    if (state === "stopped") return;
    state = "stopped";
    cancelRecheck();
    window.removeEventListener("storage", onStorage);
    stopNotes();
    stopSent();
    service.stop();
    pending?.close();
    pending = null;
  };
  window.addEventListener("pagehide", stop, { once: true });

  /** Check the keys, then start the request service, or say why not. */
  const check = async (): Promise<void> => {
    const found = await options.verify();
    if (state !== "checking") return;
    problem = found;
    if (found?.again === true) {
      // The chain could not be read: Contact Chat waits, holding its port, and checks again.
      options.log(
        `${options.product.name}'s requests wait: ${found.reason} Checking again in ${VERIFY_RETRY_MS / 1000} s.`,
      );
      cancelRecheck = schedule(() => void check(), VERIFY_RETRY_MS);
    } else if (found !== null) {
      options.log(
        `${options.product.name}'s requests stay off: ${found.reason}`,
      );
      state = "off";
      pending?.close();
      pending = null;
    } else {
      options.log(`${options.product.name}'s requests are on.`);
      state = "running";
      service.start();
      sendersChanged();
      if (pending !== null) service.attachProduct(productPort(pending));
      pending = null;
    }
    startChanged();
  };

  return {
    product: options.product,
    chatOptions: {
      contacts: handleContacts,
      onNamesChange: (listener) => {
        const stops = [
          handles.onChange(listener),
          options.onNamesChange?.(listener) ?? (() => {}),
        ];
        return () => stops.forEach((each) => each());
      },
      start: {
        // Someone chosen in New chat by username has no handle yet: the
        // host names them, and the core mints it.
        target: async (account) => {
          let handle = handleContacts.handleFor(account);
          if (handle === null && contacts.nameOf(account) !== undefined) {
            starting.add(account.toLowerCase());
            sendersChanged();
            handle = await handleOnceNamed(account, NAMING_TIMEOUT_MS);
          }
          if (handle === null)
            throw new Error("Contact Chat cannot reach this person.");
          return { handle };
        },
        blocked: () => {
          if (state === "running") return null;
          if (state === "checking")
            return problem === null
              ? "Contact Chat is checking your identity on chain."
              : `Contact Chat could not check your identity yet, and tries again shortly: ${problem.reason}`;
          return `${options.product.name}'s requests are off${problem === null ? "." : `: ${problem.reason}`}`;
        },
        onChange: (listener) => {
          startListeners.add(listener);
          return () => startListeners.delete(listener);
        },
      },
    },
    attachChat: (port) => {
      if (state === "running") service.attachProduct(productPort(port));
      else if (state === "checking") pending = port;
      else port.close();
    },
    detachChat: () => {
      pending = null;
      service.detachProduct();
    },
    owns: (id) => service.owns(id),
    accept: (item) => service.requestAccept(item.id),
    open: (item) => {
      if (item.state.tag === "accepted") service.requestOpen(item.id);
    },
    people,
    sent: () =>
      sentRequests(service, options.product, (account) =>
        contacts.nameOf(account),
      ),
    cancel: (id) =>
      service.outbox.get(id) !== undefined && service.requestCancel(id).ok,
    onSentChange: (listener) => service.onOutgoing(listener),
    start: check,
    stop,
  };
}

/** The pending requests `service` sent for `product`, newest first, named by the host. */
export function sentRequests(
  service: InvitationService,
  product: Product,
  nameOf: (account: string) => { name: string; handle: string } | undefined,
): SentRequest[] {
  return service.outbox
    .pending()
    .map((item) => {
      const person = nameOf(item.context.to);
      return {
        id: item.handle,
        to: {
          name: person?.name ?? "Contact",
          handle: person?.handle ?? item.context.to,
        },
        product,
        purpose: item.context.purpose,
        createdAt: item.context.createdAt,
        paused: item.paused,
      };
    })
    .sort((left, right) => right.createdAt - left.createdAt);
}
