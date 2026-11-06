/**
 * Where the dev server serves Kettle Chat's worker: a description of its app
 * archive at {@link KETTLE_CHAT_PATH} under the host's base, as for Contact
 * Chat (`../contact-chat/route.ts`). The archive holds the worker's module
 * and Kettle's WebAssembly, which `npm run build:kettle` writes to
 * {@link KETTLE_WASM_PACKAGE}. Free of other imports so the Vite config can
 * load it.
 */

/** Kettle Chat's worker description, under the host's base. */
export const KETTLE_CHAT_PATH = "kettle-chat/worker.json";

/** Where `npm run build:kettle` writes the WebAssembly and its glue, under `hosts/web`. */
export const KETTLE_WASM_PACKAGE = "kettle-chat-wasm/pkg";

/** The glue module and the WebAssembly, in the package and beside the worker's entry in its archive. */
export const KETTLE_GLUE = "kettle_chat.js";
export const KETTLE_WASM = "kettle_chat_bg.wasm";

/**
 * The relays of Kettle's log transfer over iroh: n0's public test relays,
 * the default iroh 1.4.1 ships, or one custom relay.
 */
export type IrohRelays =
  | { relays: "public" }
  | { relays: "custom"; relayUrl: string };

/** The query parameter of the worker's page that asks for the public test relays: `public`. */
export const IROH_PARAM = "iroh";
/** The query parameter of the worker's page that names one custom relay. */
export const IROH_RELAY_PARAM = "iroh-relay";

/** The worker page's parameters for `iroh`. */
export const irohParams = (iroh: IrohRelays): Record<string, string> =>
  iroh.relays === "public"
    ? { [IROH_PARAM]: "public" }
    : { [IROH_RELAY_PARAM]: iroh.relayUrl };

/**
 * What the worker page's parameters ask for: none when they name nothing,
 * and `refused` when they name something this does not read. A custom
 * relay goes first: the host never sets both.
 */
export function irohOfParams(params: URLSearchParams): {
  iroh: IrohRelays | null;
  refused: boolean;
} {
  const custom = params.get(IROH_RELAY_PARAM);
  if (custom !== null) {
    const relayUrl = relayUrlOf(custom);
    return relayUrl === null
      ? { iroh: null, refused: true }
      : { iroh: { relays: "custom", relayUrl }, refused: false };
  }
  const iroh = params.get(IROH_PARAM);
  if (iroh === null) return { iroh: null, refused: false };
  return iroh === "public"
    ? { iroh: { relays: "public" }, refused: false }
    : { iroh: null, refused: true };
}

/** Why a relay URL is refused. It does not repeat the value, which may hold a secret. */
export const BAD_RELAY_URL =
  "The relay must be an http or https URL with a host, and no user, password, path, query or fragment.";

/**
 * The relay `text` names, as a URL, or null. Every outbox publishes it, so
 * it holds a scheme, a host and a port only: no user, password, path, query
 * or fragment, written plainly or percent-encoded.
 */
export function relayUrlOf(text: string): string | null {
  // An empty query or fragment parses to "": refuse the marks themselves.
  if (/[?#]/.test(text)) return null;
  let url: URL;
  try {
    url = new URL(text.trim());
  } catch {
    return null;
  }
  const plain =
    (url.protocol === "http:" || url.protocol === "https:") &&
    url.hostname !== "" &&
    url.username === "" &&
    url.password === "" &&
    url.pathname === "/";
  return plain ? url.href : null;
}
