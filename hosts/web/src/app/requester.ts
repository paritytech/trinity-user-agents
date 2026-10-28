import { redactUrl } from "../platform/activity.js";

/** A product the host runs under a product id, as the host itself opened it. */
export interface RunningProduct {
  kind: "App" | "Widget" | "Worker";
  /** What the host opened it from: a URL or a name, or null for a worker it runs from its name. */
  address: string | null;
}

/** Whether `address` says nothing the product id does not: the id itself, or a bare URL of its host. */
function restates(productId: string, address: string): boolean {
  if (address === productId) return true;
  try {
    const url = new URL(address);
    return url.host === productId && url.pathname === "/" && url.search === "";
  } catch {
    return false;
  }
}

/**
 * The line a prompt adds under "Requested by <product id>", from what the
 * host itself knows of the products it runs under that id. The id is the
 * core's; this only says where the host got the product from, so an id such
 * as `localhost` can be told apart. Null when it would only repeat the id.
 *
 * Products that share an id are one product to the core, so the host cannot
 * say which of them asks, and says so.
 */
export function describeRequester(
  productId: string,
  running: readonly RunningProduct[],
): string | null {
  if (running.length === 0) return null;
  if (running.length > 1)
    return `${running.length} open products share this id, so the host cannot tell which one asks.`;
  const [only] = running;
  if (only!.address === null) return `${only!.kind} run by this host`;
  const address = redactUrl(only!.address);
  return restates(productId, address)
    ? null
    : `${only!.kind} opened from ${address}`;
}
