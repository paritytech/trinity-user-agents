// What every provider screen shares: the session it is for, the view the
// worker stored for it, and the way back to the worker.

import { getClientSync } from "@parity/truapi/sandbox";
import type { TrUApiClient } from "@parity/truapi";
import {
  actionKey,
  activeKey,
  fromStored,
  toStored,
  viewKey,
  type ScreenAction,
  type ScreenRoute,
  type SessionView,
} from "../../shared/channel";
import { FAUCET_PERMISSION } from "../../shared/faucet";
import "./style.css";

/** The page's TrUAPI client, or `null` outside a host. */
export const client: TrUApiClient | null = getClientSync();

/** The element with `id`, which every page declares. */
export const byId = <T extends HTMLElement = HTMLElement>(id: string) =>
  document.getElementById(id) as T;

/** Shows `message` in the page's status line. */
export function status(
  message: string,
  tone: "info" | "done" | "error" = "info",
): void {
  const line = byId("status");
  line.textContent = message;
  line.dataset.tone = tone;
}

async function read(key: string): Promise<string | undefined> {
  if (!client) return undefined;
  return client.localStorage.read({ key }).match(
    (response) => response.value,
    () => undefined,
  );
}

/** The session this screen is for: the route's query string, or the one the
 *  worker named for the route when the host dropped the query. */
export async function sessionIntent(
  route: ScreenRoute,
): Promise<string | undefined> {
  const fromQuery = new URLSearchParams(location.search).get("intent");
  return fromQuery ?? fromStored<string>(await read(activeKey(route)));
}

/** The view the worker stored for the session. */
export async function sessionView(
  intent: string,
): Promise<SessionView | undefined> {
  return fromStored<SessionView>(await read(viewKey(intent)));
}

/** Tells the worker what the user did. */
export async function act(
  intent: string,
  action: ScreenAction,
): Promise<boolean> {
  if (!client) return false;
  return client.localStorage
    .write({ key: actionKey(intent), value: toStored(action) })
    .match(
      () => true,
      () => false,
    );
}

/** Asks the user to let this provider reach the public faucet, which the
 *  worker draws the simulated payment from. Asked here because the user is on
 *  the screen to answer; the grant covers the worker as well. */
export async function allowFaucet(): Promise<boolean> {
  if (!client) return false;
  return client.permissions.requestRemotePermission(FAUCET_PERMISSION).match(
    (response) => response.granted,
    () => false,
  );
}

/** `amount` of an asset with `decimals`, as a decimal string. */
export function formatUnits(amount: string | bigint, decimals: number): string {
  const value = BigInt(amount);
  const unit = 10n ** BigInt(decimals);
  const whole = value / unit;
  const fraction = (value % unit)
    .toString()
    .padStart(decimals, "0")
    .slice(0, Math.min(decimals, 4));
  return decimals === 0
    ? whole.toString()
    : `${whole}.${fraction.padEnd(2, "0")}`;
}

/** A decimal string as CASH base units, or `undefined` when it is not one. */
export function parseCash(text: string): string | undefined {
  const match = /^\s*(\d+)(?:[.,](\d{1,6}))?\s*$/.exec(text);
  const whole = match?.[1];
  if (whole === undefined) return undefined;
  const fraction = (match?.[2] ?? "").padEnd(6, "0");
  return (BigInt(whole) * 1_000_000n + BigInt(fraction)).toString();
}

/** Loads the session for `route` and hands it to `render`, or explains why
 *  there is none. */
export async function start(
  route: ScreenRoute,
  render: (intent: string, view: SessionView | undefined) => void,
): Promise<void> {
  if (!client) {
    status(
      "Open this screen from a funding session in a Polkadot host.",
      "error",
    );
    return;
  }
  const intent = await sessionIntent(route);
  if (!intent) {
    status("No funding session is waiting for this screen.", "error");
    return;
  }
  render(intent, await sessionView(intent));
}
