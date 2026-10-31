import { blake2b } from "@noble/hashes/blake2.js";
import type {
  ContactsPlatform,
  HostContactLookup,
  HostContactMatches,
  HostContactPick,
  ProductContext,
} from "@parity/truapi-host";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type { ChatContact } from "../chat/provider.js";
import {
  ask,
  type PromptLookup,
  type PromptLookupView,
} from "../app/prompt.js";
import {
  type DiscoveryView,
  type ListedUser,
  type UsernameDirectory,
  UsernameDiscovery,
} from "./username-directory.js";

/**
 * The host side of TrUAPI Contacts: the picker a product opens with
 * `contacts.pick`, and the lookup the core makes when a transaction names a
 * contact handle.
 *
 * The picker offers the signed-in session's chat contacts, which the chat
 * providers offer (`ChatProvider.contacts`), and, where the host can search,
 * people found by username. Either way a product gets a handle for the one
 * person the user picked, minted by the core, never the list, a name or an
 * account, and decides itself what to do with it. The rules follow
 * `docs/design/contacts-pick-end-to-end.md`.
 *
 * A person found by username is not added to any list. The host remembers
 * them in this page only, so a handle the product holds keeps naming them
 * after the core forgets its cached handles; after a reload, or once the
 * session ends or the user blocks them, it names nobody.
 *
 * To draw a handle a product holds as the person it names, the host keeps an
 * index of the handles the core mints for the people it may name
 * ({@link HostContactsDeps.mint}). The index is for naming only: whom a
 * transaction pays is still decided by {@link HostContacts.platform}'s lookup
 * over the contacts alone.
 */

/** Whom the user chose in the picker. */
export interface PickedPerson {
  account: HexString;
  /** How the picker named them: a contact's name, or the username they were found by. Host only. */
  name: string;
  /** True for a person found by username rather than one of the contacts. */
  found: boolean;
}

/** Who opens the picker: a product, or the host itself. */
export interface PickerAsker {
  title: string;
  /** The product that asks, named on the sheet. Absent when the host asks for itself. */
  from?: string;
}

export interface HostContactsDeps {
  /** The session's contacts now. Empty when no one is signed in. */
  list(): readonly ChatContact[];
  /** Calls back whenever the list may have changed. */
  onChange(listener: () => void): () => void;
  /**
   * Show the picker for `asker` and resolve with whom the user chose, or null
   * when they closed it. `blocked` people are not offered.
   */
  choose(
    asker: PickerAsker,
    contacts: readonly ChatContact[],
    blocked: ReadonlySet<string>,
  ): Promise<PickedPerson | null>;
  /** The accounts the user blocked. A blocked person is never picked or resolved. */
  blocked(): readonly HexString[];
  /** Calls back when the signed-in session ends or changes. */
  onSessionChange(listener: () => void): () => void;
  /** Tell the core someone left, so a handle it cached stops resolving. */
  removed(): void;
  /**
   * The handles the core knows `accounts` by under its signed-in session, in
   * order: the ones `contacts.pick` gives products. Undefined without a
   * session. The core caches none of them, so naming someone never makes a
   * transaction resolve them.
   */
  mint(
    accounts: readonly HexString[],
  ): Promise<readonly HexString[] | undefined>;
  /**
   * People the host may draw by name beyond its contacts, such as everyone a
   * test user's fixture directory knows. They are never offered in the picker
   * and never resolved for a transaction.
   */
  named?(): readonly HexString[];
  /** Run `task` after `delayMs`; answers a cancel. Defaults to `setTimeout`. */
  schedule?(task: () => void, delayMs: number): () => void;
  log?(line: string): void;
}

/**
 * Where the handle index stands. `ready` holds the core's answer for the
 * current session and people, even when that names nobody; `failed` means
 * the core gave none after every retry. Either of the other two names only
 * people a previous answer named and who are still there.
 */
export type HandleIndexStatus = "signed-out" | "pending" | "ready" | "failed";

/** How long to wait before each new mint after one failed, in order. Then the index waits for the next change. */
export const MINT_RETRY_MS: readonly number[] = [500, 2_000, 8_000];

/** The host's names for contact handles, for a feature that draws them. */
export interface HandleNames {
  /** The account `handle` names among the people the host may name now, or null. */
  accountFor(handle: string): HexString | null;
  /** The handle the person with `account` is known by, or null when the host may not name them now. */
  handleFor(account: string): HexString | null;
  /** Calls back whenever the answers above may have changed. */
  onChange(listener: () => void): () => void;
}

/**
 * The handle the core knows `account` by under `handleKey`: BLAKE2b-256
 * keyed with the handle key, over the account's 32 bytes.
 */
export function contactHandle(
  handleKey: HexString,
  account: HexString,
): HexString {
  return bytesToHex(
    blake2b(hexToBytes(account), { key: hexToBytes(handleKey), dkLen: 32 }),
  ) as HexString;
}

/** Contacts with a last message, newest first, then the rest as listed. */
export function byRecent(contacts: readonly ChatContact[]): ChatContact[] {
  return contacts
    .map((contact, index) => ({ contact, index }))
    .sort(
      (left, right) =>
        (right.contact.lastActiveAt ?? -1) -
          (left.contact.lastActiveAt ?? -1) || left.index - right.index,
    )
    .map(({ contact }) => contact);
}

const sameAccount = (left: string, right: string): boolean =>
  left.toLowerCase() === right.toLowerCase();

export class HostContacts implements HandleNames {
  private known = new Set<string>();
  /** People found by username and picked this session, by lower-case account. */
  private picked = new Map<string, HexString>();
  /** Counts sessions, so a choice made in one is not an answer in the next. */
  private session = 0;
  /** Whom each handle names, by lower-case handle. */
  private byHandle = new Map<string, HexString>();
  /** Each named person's handle, by lower-case account. */
  private byAccount = new Map<string, HexString>();
  /** The accounts the last mint asked for, so a change that names no one new asks again for nothing. */
  private indexed: string | null = null;
  /** Counts every change a mint answer may not outlive: a session, a person gone, a new list. */
  private indexVersion = 0;
  /** The core session the index is for, by its public key. */
  private coreSession: string | null = null;
  private indexStatus: HandleIndexStatus = "signed-out";
  private cancelRetry: () => void = () => {};
  private readonly handleListeners = new Set<() => void>();

  constructor(private readonly deps: HostContactsDeps) {
    this.known = this.accounts();
    deps.onChange(() => {
      this.follow();
      this.reindex(false);
    });
    deps.onSessionChange(() => {
      this.session += 1;
      this.picked.clear();
      this.deps.removed();
      this.reindex(true);
    });
  }

  /**
   * Follow the core's signed-in session, by its public key, or null when no
   * one is signed in. A new session drops every handle, since each is minted
   * under the session's key.
   */
  followSession(key: string | null): void {
    if (key === this.coreSession) return;
    this.coreSession = key;
    this.reindex(true);
  }

  /** Where the handle index stands now. */
  status(): HandleIndexStatus {
    return this.indexStatus;
  }

  /** The people {@link HostContactsDeps.named} gives may have changed: ask the core for handles again if so. */
  namesChanged(): void {
    this.reindex(false);
  }

  accountFor(handle: string): HexString | null {
    return this.byHandle.get(handle.toLowerCase()) ?? null;
  }

  handleFor(account: string): HexString | null {
    return this.byAccount.get(account.toLowerCase()) ?? null;
  }

  onChange(listener: () => void): () => void {
    this.handleListeners.add(listener);
    return () => this.handleListeners.delete(listener);
  }

  /** Everyone the host may name now, once each: the resolvable, then the other people it knows, less the blocked. */
  private named(): HexString[] {
    const blocked = this.blocked();
    const seen = new Set<string>();
    return [...this.resolvable(), ...(this.deps.named?.() ?? [])].filter(
      (account) => {
        const key = account.toLowerCase();
        if (blocked.has(key) || seen.has(key)) return false;
        seen.add(key);
        return true;
      },
    );
  }

  /**
   * Ask the core for the handles of everyone the host may name. Someone no
   * longer named leaves the index at once, without waiting for the answer;
   * `fresh` empties it, for a new session. An answer that a later change
   * overtook is dropped, and so is a retry it scheduled.
   */
  private reindex(fresh: boolean): void {
    const accounts = this.named();
    const key = accounts.map((account) => account.toLowerCase()).join(",");
    if (!fresh && key === this.indexed) return;
    const version = ++this.indexVersion;
    this.cancelRetry();
    this.indexed = key;
    const keep = new Set(key.split(","));
    let dropped = false;
    for (const [handle, account] of this.byHandle)
      if (fresh || !keep.has(account.toLowerCase())) {
        this.byHandle.delete(handle);
        this.byAccount.delete(account.toLowerCase());
        dropped = true;
      }
    this.indexStatus = this.coreSession === null ? "signed-out" : "pending";
    if (dropped) this.handlesChanged();
    if (this.coreSession !== null) this.mint(accounts, version, 0);
  }

  /** One mint for `version`, the `attempt`th since it started. */
  private mint(
    accounts: readonly HexString[],
    version: number,
    attempt: number,
  ): void {
    const failed = (reason: string) => {
      if (version !== this.indexVersion) return;
      const delay = MINT_RETRY_MS[attempt];
      if (delay === undefined) {
        this.indexed = null;
        this.indexStatus = "failed";
        this.deps.log?.(`The core minted no contact handles: ${reason}`);
        this.handlesChanged();
        return;
      }
      const schedule =
        this.deps.schedule ??
        ((task: () => void, ms: number) => {
          const timer = setTimeout(task, ms);
          return () => clearTimeout(timer);
        });
      this.cancelRetry = schedule(() => {
        if (version === this.indexVersion)
          this.mint(accounts, version, attempt + 1);
      }, delay);
    };
    this.deps.mint(accounts).then(
      (handles) => {
        if (version !== this.indexVersion) return;
        // Signed in here, so no answer means the core is not there yet.
        if (handles === undefined) return failed("the core has no session");
        if (handles.length !== accounts.length)
          return failed(
            `${handles.length} handles for ${accounts.length} accounts`,
          );
        this.byHandle.clear();
        this.byAccount.clear();
        accounts.forEach((account, index) => {
          const handle = handles[index]!.toLowerCase() as HexString;
          this.byHandle.set(handle, account);
          this.byAccount.set(account.toLowerCase(), handle);
        });
        this.indexStatus = "ready";
        this.handlesChanged();
      },
      (error: unknown) => failed(String(error)),
    );
  }

  private handlesChanged(): void {
    for (const listener of [...this.handleListeners]) listener();
  }

  private blocked(): Set<string> {
    return new Set(this.deps.blocked().map((account) => account.toLowerCase()));
  }

  /** Everyone a handle may name now: the contacts and the people picked by username, less the blocked. */
  private resolvable(): HexString[] {
    const blocked = this.blocked();
    return [
      ...this.deps.list().map((contact) => contact.account),
      ...this.picked.values(),
    ].filter((account) => !blocked.has(account.toLowerCase()));
  }

  private accounts(): Set<string> {
    return new Set(
      this.deps.list().map((contact) => contact.account.toLowerCase()),
    );
  }

  /**
   * Signal the core when anyone it may hold a handle for is gone: a contact
   * who left, or a person picked by username who is now blocked. A contact
   * who left is forgotten as a picked person too.
   */
  private follow(): void {
    const now = this.accounts();
    const gone = [...this.known].filter((account) => !now.has(account));
    const blocked = this.blocked();
    for (const account of this.picked.keys())
      if (blocked.has(account)) gone.push(account);
    for (const account of gone) this.picked.delete(account);
    this.known = now;
    if (gone.length > 0) this.deps.removed();
  }

  /** The platform for `createHostCallbacks`. Both calls are given, so the core serves `contacts.pick`. */
  readonly platform: Required<ContactsPlatform> = {
    contacts: async (
      lookup: HostContactLookup,
    ): Promise<HostContactMatches> => {
      const byHandle = new Map(
        this.resolvable().map((account) => [
          contactHandle(lookup.handleKey, account),
          account,
        ]),
      );
      return {
        accounts: lookup.handles.map(
          (handle) =>
            byHandle.get(handle.toLowerCase() as HexString) ?? undefined,
        ),
      };
    },
    pickContact: async (product: ProductContext): Promise<HostContactPick> => {
      // The picker opens even with no contacts: anyone can be found by username.
      this.deps.log?.(
        `${product.productId} opened the contact picker; it gets a handle for the person chosen, not their name, account or the list.`,
      );
      const chosen = await this.choose({
        title: "Choose a contact",
        from: product.productId,
      });
      if (chosen === null) return { tag: "Dismissed" };
      if (chosen.found) {
        this.picked.set(chosen.account.toLowerCase(), chosen.account);
        this.reindex(false);
      }
      return { tag: "Picked", value: { account: chosen.account } };
    },
  };

  /**
   * The picker, for the host's own use, such as Chat's New chat. It answers
   * whom the user chose and mints no handle; a product's pick goes through
   * {@link platform} instead. `others` adds people the host offers here
   * only, such as the people of a chat product's rooms, never to a product. Null when the user closed it, the session
   * changed meanwhile, or the person chosen is blocked or no longer a contact.
   */
  async choose(
    asker: PickerAsker,
    others: () => readonly ChatContact[] = () => [],
  ): Promise<PickedPerson | null> {
    const session = this.session;
    const offered = () => {
      const contacts = this.deps.list();
      const listed = new Set(
        contacts.map((contact) => contact.account.toLowerCase()),
      );
      return [
        ...contacts,
        ...others().filter((other) => !listed.has(other.account.toLowerCase())),
      ];
    };
    const chosen = await this.deps.choose(asker, offered(), this.blocked());
    if (chosen === null || session !== this.session) return null;
    if (this.blocked().has(chosen.account.toLowerCase())) return null;
    if (chosen.found) return chosen;
    // The list can change while the picker is open; only a current contact is an answer.
    const current = offered().find((contact) =>
      sameAccount(contact.account, chosen.account),
    );
    return current === undefined
      ? null
      : { ...chosen, account: current.account };
  }
}

/**
 * The picker: the user's contacts, the ones chatted with most recently first,
 * one row each, named as the host names them, with a search field over the
 * names, and Cancel. The product learns only the outcome, never what was
 * shown.
 *
 * What is typed is also looked up by username in `directory`, and the people
 * found who are neither contacts nor blocked show in a group of their own. Choosing
 * one checks the username on chain first ({@link UsernameDirectory.confirm});
 * a person whose username names another account there is not picked.
 */
export function chooseContact(
  asker: PickerAsker,
  contacts: readonly ChatContact[],
  directory: UsernameDirectory,
  blocked: ReadonlySet<string> = new Set(),
): Promise<PickedPerson | null> {
  return ask<PickedPerson | null>({
    title: asker.title,
    ...(asker.from === undefined ? {} : { from: asker.from }),
    fields: [],
    layout: "list",
    search: {
      label: "Search contacts",
      placeholder: "Search by name or username",
      empty: "No contact by that name.",
      lookup: usernameLookup(directory, contacts, blocked),
    },
    choices: [
      ...byRecent(contacts).map((contact) => ({
        label: contact.name,
        value: { account: contact.account, name: contact.name, found: false },
        avatarSeed: contact.account,
      })),
      { label: "Cancel", value: null, tone: "cancel" },
    ],
    dismissed: null,
  });
}

/** What the picker shows for a username search's state. */
export function lookupView(
  view: DiscoveryView,
  choose: (user: ListedUser) => void,
  checking: string | null = null,
): PromptLookupView {
  const rows = (users: readonly ListedUser[]) =>
    users.map((user) => ({
      label: user.username,
      avatarSeed: user.account,
      choose: () => choose(user),
    }));
  const shown = (
    users: readonly ListedUser[],
    status: string | null,
    more = false,
  ): PromptLookupView => ({
    visible: true,
    rows: rows(users),
    enabled: checking === null,
    status: checking ?? status,
    more: checking === null && more,
  });
  switch (view.tag) {
    case "idle":
      return {
        visible: false,
        rows: [],
        enabled: true,
        status: null,
        more: false,
      };
    case "invalid":
      return shown(
        [],
        "Usernames are letters and digits, like alice or alice.06.",
      );
    case "searching":
      return shown(view.users, "Searching usernames…");
    case "found":
      return shown(
        view.users,
        view.users.length === 0 && !view.more
          ? "No one else by that username."
          : null,
        view.more,
      );
    case "failed":
      return shown(view.users, view.message);
  }
}

/** The picker's username group: search as the user types, and check a person on chain before picking them. */
export function usernameLookup(
  directory: UsernameDirectory,
  contacts: readonly ChatContact[],
  blocked: ReadonlySet<string> = new Set(),
): PromptLookup<PickedPerson | null> {
  const known = new Set([
    ...contacts.map((contact) => contact.account.toLowerCase()),
    ...blocked,
  ]);
  return {
    ownHeading: "Contacts",
    heading: "Not in your contacts",
    note: `Found by username in ${directory.source}.`,
    open: (render, answer) => {
      let view: DiscoveryView = { tag: "idle" };
      let checking: string | null = null;
      let refused: string | null = null;
      let closed = false;
      const draw = () => {
        const shown = lookupView(view, choose, checking);
        render(refused === null ? shown : { ...shown, status: refused });
      };
      const choose = (user: ListedUser) => {
        if (checking !== null || closed) return;
        checking = `Checking ${user.username} on chain…`;
        refused = null;
        draw();
        void directory.confirm(user).then((problem) => {
          if (closed) return;
          checking = null;
          if (problem === null)
            return answer({
              account: user.account,
              name: user.username,
              found: true,
            });
          refused = problem;
          draw();
        });
      };
      const discovery = new UsernameDiscovery({
        directory,
        known: (account) => known.has(account.toLowerCase()),
        show: (next) => {
          view = next;
          refused = null;
          draw();
        },
      });
      return {
        type: (text) => discovery.type(text),
        more: () => discovery.more(),
        close: () => {
          closed = true;
          discovery.close();
        },
      };
    },
  };
}
