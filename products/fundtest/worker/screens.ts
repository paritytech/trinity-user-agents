// Presenting the provider's screens and hearing back from them.
//
// `presentFrame` answers only once the frame closes, and nothing in TrUAPI
// lets a screen close its own frame, so the worker does not wait for it: the
// screen writes what the user did to the product's storage, and the worker
// takes that as soon as it lands. A frame that closes before the screen wrote
// anything was left by the user.

import {
  actionKey,
  activeKey,
  fromStored,
  killedKey,
  screenUrl,
  toStored,
  viewKey,
  type ScreenAction,
  type ScreenRoute,
  type SessionView,
} from "../shared/channel";
import { firstOf, truapi, unwrap } from "./host";

/** Stores what a session's screens show. */
export async function writeView(view: SessionView): Promise<void> {
  const client = await truapi();
  await unwrap(
    client.localStorage.write({
      key: viewKey(view.intent),
      value: toStored(view),
    }),
    "store the view",
  );
}

async function readAction(intent: string): Promise<ScreenAction | undefined> {
  const client = await truapi();
  const { value } = await unwrap(
    client.localStorage.read({ key: actionKey(intent) }),
    "read the action",
  );
  return fromStored<ScreenAction>(value);
}

/**
 * Shows the screen at `route` for `intent` and resolves with the user's action
 * of kind `kind`, or `undefined` when the user closed the screen without one.
 * An action stored before a restart is taken without presenting again.
 */
export async function presentScreen<K extends ScreenAction["kind"]>(
  intent: string,
  route: ScreenRoute,
  kind: K,
  signal: AbortSignal,
): Promise<Extract<ScreenAction, { kind: K }> | undefined> {
  type Wanted = Extract<ScreenAction, { kind: K }>;
  const isWanted = (action: ScreenAction | undefined): action is Wanted =>
    action?.kind === kind;
  const stored = await readAction(intent);
  if (isWanted(stored)) return stored;

  const client = await truapi();
  await unwrap(
    client.localStorage.write({
      key: activeKey(route),
      value: toStored(intent),
    }),
    "store the active session",
  );
  const listening = new AbortController();
  const stop = () => listening.abort();
  signal.addEventListener("abort", stop, { once: true });
  const acted = firstOf(
    client.localStorage.subscribe({ request: { key: actionKey(intent) } }),
    (change) => {
      const action = fromStored<ScreenAction>(change.value);
      return isWanted(action) ? action : undefined;
    },
    listening.signal,
  );
  const closed = unwrap(
    client.fundingProvider.presentFrame({
      intent,
      target: { tag: "App", value: { route: screenUrl(route, intent) } },
    }),
    `present ${route}`,
  ).then(async (response) => {
    const action = await readAction(intent);
    if (isWanted(action)) return action;
    // The host's own "I've sent funds" closes the bank screen with Closed:
    // the user saying they paid, as the page's own button does.
    if (kind === "bank-sent" && response.outcome.tag === "Closed") {
      return { kind: "bank-sent" } as Wanted;
    }
    return undefined;
  });
  try {
    // Whichever settles first: the action, or the frame closing without one.
    return await Promise.race([acted, closed]);
  } finally {
    signal.removeEventListener("abort", stop);
    stop();
    acted.catch(() => undefined);
    closed.catch((error: unknown) =>
      console.warn(`[fundtest] ${route} frame: ${String(error)}`),
    );
  }
}

/** Drops what a session left in the product's storage. */
export async function forgetSession(intent: string): Promise<void> {
  const client = await truapi();
  for (const key of [viewKey(intent), actionKey(intent), killedKey(intent)]) {
    await client.localStorage.clear({ key }).match(
      () => undefined,
      (error) =>
        console.warn(`[fundtest] could not clear ${key}: ${String(error)}`),
    );
  }
}

/** Whether the session stopped at its kill point before. */
export async function wasKilled(intent: string): Promise<boolean> {
  const client = await truapi();
  const { value } = await unwrap(
    client.localStorage.read({ key: killedKey(intent) }),
    "read the kill mark",
  );
  return fromStored<boolean>(value) === true;
}

/** Marks that the session stopped at its kill point. */
export async function markKilled(intent: string): Promise<void> {
  const client = await truapi();
  await unwrap(
    client.localStorage.write({
      key: killedKey(intent),
      value: toStored(true),
    }),
    "store the kill mark",
  );
}
