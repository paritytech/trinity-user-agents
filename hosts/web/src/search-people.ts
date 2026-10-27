import type { HexString } from "@parity/truapi/scale";
import type { ChatContact } from "./chat-provider.js";
import type { PickedPerson } from "./contacts.js";
import {
  UsernameDiscovery,
  type DiscoveryView,
  type ListedUser,
  type UsernameDirectory,
} from "./username-directory.js";

/**
 * The people the Docklet's search offers beside apps: the session's
 * contacts whose names match what is typed, then people found by username in
 * the same directory the contact picker searches. Nothing is chosen while
 * typing; a row answers only when the user picks it, and a person found by
 * username is checked on chain first, as in the picker.
 */

export interface SearchPeopleDeps {
  /** Whether a chosen person can be shown in Chat now. When not, no person is offered. */
  available(): boolean;
  /** The session's contacts, as the host names them. */
  contacts(): readonly ChatContact[];
  blocked(): readonly HexString[];
  directory(): UsernameDirectory;
  /** Something to draw changed. */
  changed(): void;
  /** The user picked this person. */
  chose(person: PickedPerson): void;
  /** How long typing must pause before a username search is sent. */
  delayMs?: number;
}

export interface PersonRow {
  key: string;
  label: string;
  /** Under the label, for a person who is not a contact. */
  note: string | null;
  avatarSeed: string;
  choose(): void;
}

const MAX_CONTACTS = 5;

export class SearchPeople {
  private text = "";
  private discovery: UsernameDiscovery | null = null;
  private view: DiscoveryView = { tag: "idle" };
  private checking: string | null = null;
  private refused: string | null = null;
  /** Counts resets, so a check that ends after one answers nothing. */
  private generation = 0;

  constructor(private readonly deps: SearchPeopleDeps) {}

  /** Follow what is typed. A person still being checked is let go: the user moved on. */
  type(text: string): void {
    this.text = text;
    this.generation += 1;
    this.checking = null;
    this.refused = null;
    if (!this.deps.available()) return this.reset();
    this.discovery ??= new UsernameDiscovery({
      directory: this.deps.directory(),
      known: (account) => this.known(account),
      show: (view) => {
        this.view = view;
        this.deps.changed();
      },
      ...(this.deps.delayMs === undefined
        ? {}
        : { delayMs: this.deps.delayMs }),
    });
    this.discovery.type(text);
  }

  /** Forget what was typed and found, as when the card closes or the session changes. */
  reset(): void {
    this.generation += 1;
    this.discovery?.close();
    this.discovery = null;
    this.view = { tag: "idle" };
    this.checking = null;
    this.refused = null;
  }

  /** Whether the People group shows: something was typed that can name a person. */
  shown(): boolean {
    return (
      this.deps.available() &&
      this.text.trim() !== "" &&
      this.view.tag !== "idle" &&
      this.view.tag !== "invalid"
    );
  }

  rows(): PersonRow[] {
    if (!this.shown()) return [];
    const contacts = this.matchingContacts().map((contact) =>
      this.row(contact.account, contact.name, null, () =>
        this.deps.chose({
          account: contact.account,
          name: contact.name,
          found: false,
        }),
      ),
    );
    const found =
      this.view.tag === "idle" || this.view.tag === "invalid"
        ? []
        : this.view.users.map((user) =>
            this.row(user.account, user.username, "Not in your contacts", () =>
              this.check(user),
            ),
          );
    return [...contacts, ...found];
  }

  /** A line under the people: a search under way, why one failed, or why a person was not picked. */
  status(): string | null {
    if (!this.shown()) return null;
    if (this.checking !== null) return this.checking;
    if (this.refused !== null) return this.refused;
    if (this.view.tag === "searching") return "Searching usernames…";
    if (this.view.tag === "failed") return this.view.message;
    return null;
  }

  private row(
    account: string,
    label: string,
    note: string | null,
    choose: () => void,
  ): PersonRow {
    return {
      key: `person:${account.toLowerCase()}`,
      label,
      note,
      avatarSeed: account,
      choose: () => {
        if (this.checking === null) choose();
      },
    };
  }

  private blocked(): Set<string> {
    return new Set(this.deps.blocked().map((account) => account.toLowerCase()));
  }

  private known(account: string): boolean {
    const id = account.toLowerCase();
    return (
      this.blocked().has(id) ||
      this.deps
        .contacts()
        .some((contact) => contact.account.toLowerCase() === id)
    );
  }

  private matchingContacts(): ChatContact[] {
    const query = this.text.trim().toLowerCase();
    const blocked = this.blocked();
    return this.deps
      .contacts()
      .filter(
        (contact) =>
          !blocked.has(contact.account.toLowerCase()) &&
          contact.name.toLowerCase().includes(query),
      )
      .slice(0, MAX_CONTACTS);
  }

  private check(user: ListedUser): void {
    const generation = this.generation;
    this.checking = `Checking ${user.username} on chain…`;
    this.refused = null;
    this.deps.changed();
    void this.deps
      .directory()
      .confirm(user)
      .then((problem) => {
        if (generation !== this.generation) return;
        this.checking = null;
        const blocked = this.blocked().has(user.account.toLowerCase());
        if (problem === null && !blocked)
          return this.deps.chose({
            account: user.account,
            name: user.username,
            found: true,
          });
        this.refused = problem ?? "You blocked this person.";
        this.deps.changed();
      });
  }
}
