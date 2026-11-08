import type { AuthState } from "@parity/truapi-host";
import type { Wallet } from "../platform/wallets.js";

/** The account the core reports as signed in. */
export type ConnectedAccount = Extract<
  AuthState,
  { tag: "Connected" }
>["value"];

/**
 * Who this tab acts as: the signed-in wallet, or a session the dev server's
 * tools run in its place (`tools/demos/dev-tools.ts`). The page asks it
 * whatever depends on who that is, and hands it each session change.
 */
export interface TabSession {
  /** The account picker's id for the session signed in now, or null. */
  activeId(): string | null;
  /** Sign the tab in to `wallet`. */
  signIn(wallet: Wallet): void;
  signOut(): void;
  /** The name the session line gives `account`. */
  name(account: ConnectedAccount): string;
  /** What copied evidence says of the session: an id to tell sessions apart, and who it is, naming no wallet. */
  evidence(): { id: string; who: string };
  /** Hand what belongs to the session the account the core reports, or none. */
  sync(account: ConnectedAccount | null): void;
  /** Once the core runs, sign the tab in as it was before the reload. */
  resume(): Promise<void>;
  /**
   * Whether the tab opens products the user did not choose this time: a dock
   * destination's own, and the one in front before a reload.
   */
  readonly autoOpens: boolean;
  /** What the empty stage suggests. */
  readonly emptyStageNote: string;
  /** What Chat says while no chat product runs, or null for its own words. */
  readonly noChat: { title: string; body: string } | null;
}
