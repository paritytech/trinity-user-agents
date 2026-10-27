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
import type { ChatContact } from "./chat-provider.js";
import { ask } from "./prompt.js";

/**
 * The host side of TrUAPI Contacts: the picker a product opens with
 * `contacts.pick`, and the lookup the core makes when a transaction names a
 * contact handle.
 *
 * The contacts are the signed-in session's chat contacts, which the chat
 * providers offer (`ChatProvider.contacts`). The list stays in this page: a
 * product gets a handle for the one person the user picked, minted by the
 * core, never the list, a name or an account. The rules follow
 * `docs/design/contacts-pick-end-to-end.md`.
 */

export interface HostContactsDeps {
  /** The session's contacts now. Empty when no one is signed in. */
  list(): readonly ChatContact[];
  /** Calls back whenever the list may have changed. */
  onChange(listener: () => void): () => void;
  /**
   * Show the picker for `productId` and resolve with the account the user
   * chose, or null when they closed it.
   */
  choose(
    productId: string,
    contacts: readonly ChatContact[],
  ): Promise<HexString | null>;
  /** Tell the core a contact left, so a handle it cached stops resolving. */
  removed(): void;
  log?(line: string): void;
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

const sameAccount = (left: string, right: string): boolean =>
  left.toLowerCase() === right.toLowerCase();

export class HostContacts {
  private known = new Set<string>();

  constructor(private readonly deps: HostContactsDeps) {
    this.known = this.accounts();
    deps.onChange(() => this.follow());
  }

  private accounts(): Set<string> {
    return new Set(
      this.deps.list().map((contact) => contact.account.toLowerCase()),
    );
  }

  /** Signal the core when any contact it may hold a handle for is gone. */
  private follow(): void {
    const now = this.accounts();
    const left = [...this.known].some((account) => !now.has(account));
    this.known = now;
    if (left) this.deps.removed();
  }

  /** The platform for `createHostCallbacks`. Both calls are given, so the core serves `contacts.pick`. */
  readonly platform: Required<ContactsPlatform> = {
    contacts: async (
      lookup: HostContactLookup,
    ): Promise<HostContactMatches> => {
      const byHandle = new Map(
        this.deps
          .list()
          .map((contact) => [
            contactHandle(lookup.handleKey, contact.account),
            contact.account,
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
      const contacts = this.deps.list();
      if (contacts.length === 0) return { tag: "NoContacts" };
      this.deps.log?.(
        `${product.productId} opened the contact picker; it gets a handle for the person chosen, not their name, account or the list.`,
      );
      const chosen = await this.deps.choose(product.productId, contacts);
      // The list can change while the picker is open; only a current contact is an answer.
      const current = this.deps
        .list()
        .find(
          (contact) => chosen !== null && sameAccount(contact.account, chosen),
        );
      return current === undefined
        ? { tag: "Dismissed" }
        : { tag: "Picked", value: { account: current.account } };
    },
  };
}

/**
 * The picker: one row per contact, named as the host names them, and Cancel.
 * The product learns only the outcome, never what was shown.
 */
export function chooseContact(
  productId: string,
  contacts: readonly ChatContact[],
): Promise<HexString | null> {
  return ask<HexString | null>({
    title: "Choose a contact",
    from: productId,
    fields: [],
    layout: "list",
    choices: [
      ...contacts.map((contact) => ({
        label: contact.name,
        value: contact.account,
        avatarSeed: contact.account,
      })),
      { label: "Cancel", value: null, tone: "cancel" },
    ],
    dismissed: null,
  });
}
