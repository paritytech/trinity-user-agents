/**
 * The identity backend's public username search, answered on the host's own
 * origin by the dev server (`username-search-plugin.ts`). The backend sends no
 * CORS headers, so a page cannot read it directly. A build has no such route.
 *
 * Only the search path of the chosen network's own backend is reached, with
 * only the search parameters and no headers from the page. The backend's own
 * validation, rate limit and errors answer as they are.
 */

import { NETWORKS, type NetworkConfig } from "./network-config.js";

/** Where the dev server answers username searches, under the host's base. */
export const USERNAME_SEARCH_PATH = "__username-search";

/** The backend's search route, on the network's `identityBackend` origin. */
const UPSTREAM_PATH = "/api/v1/usernames/search";

/** The search parameters passed on; the backend owns their rules. */
const FORWARDED = ["prefix", "limit", "cursor"];

/** Longest value passed on. Real cursors are a few hundred characters. */
const MAX_VALUE_LENGTH = 2048;

const TIMEOUT_MS = 10_000;

/** Largest answer passed on. A full page of 1,000 usernames is about 250 KiB. */
const MAX_BODY_BYTES = 1024 * 1024;

/** Sends one request to the backend. */
export type Fetcher = (request: Request) => Promise<Response>;

function failure(status: number, error: string): Response {
  return Response.json(
    { error },
    { status, headers: { "cache-control": "no-store" } },
  );
}

/** The answer for a request that failed or timed out, including while its body was read. */
function unreachable(error: unknown, network: NetworkConfig): Response {
  return error instanceof DOMException && error.name === "TimeoutError"
    ? failure(
        504,
        `The ${network.displayName} identity backend did not answer in time.`,
      )
    : failure(
        502,
        `The ${network.displayName} identity backend is unreachable.`,
      );
}

/** The answer's bytes, or null when it is larger than {@link MAX_BODY_BYTES}. */
async function readBounded(
  response: Response,
): Promise<Uint8Array<ArrayBuffer> | null> {
  if (response.body === null) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > MAX_BODY_BYTES) {
      await reader.cancel();
      return null;
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return bytes;
}

/**
 * Answer one search request for `url`, whose query names the `network` and
 * the backend's `prefix`, `limit` and `cursor`. Every failure is an error
 * status with an `error` message, never an empty page.
 */
export async function searchUsernames(
  method: string,
  url: URL,
  fetcher: Fetcher = fetch,
  networks: Record<string, NetworkConfig> = NETWORKS,
): Promise<Response> {
  if (method !== "GET") {
    const refused = failure(405, "Username search answers GET only.");
    refused.headers.set("allow", "GET");
    return refused;
  }

  const query = url.searchParams;
  for (const name of new Set(query.keys())) {
    if (name !== "network" && !FORWARDED.includes(name))
      return failure(400, `Unknown parameter "${name}".`);
    if (query.getAll(name).length > 1)
      return failure(400, `Parameter "${name}" is given more than once.`);
    if (query.get(name)!.length > MAX_VALUE_LENGTH)
      return failure(400, `Parameter "${name}" is too long.`);
  }

  const networkId = query.get("network");
  if (networkId === null)
    return failure(400, `Parameter "network" is required.`);
  if (!Object.hasOwn(networks, networkId))
    return failure(400, `Unknown network "${networkId}".`);
  const network = networks[networkId];
  if (network.identityBackend === null)
    return failure(503, `${network.displayName} names no identity backend.`);

  const upstream = new URL(UPSTREAM_PATH, network.identityBackend);
  for (const name of FORWARDED) {
    const value = query.get(name);
    if (value !== null) upstream.searchParams.set(name, value);
  }

  let response: Response;
  try {
    response = await fetcher(
      new Request(upstream, {
        headers: { accept: "application/json" },
        redirect: "manual",
        signal: AbortSignal.timeout(TIMEOUT_MS),
      }),
    );
  } catch (error) {
    return unreachable(error, network);
  }

  if (
    response.type === "opaqueredirect" ||
    (response.status >= 300 && response.status < 400)
  ) {
    await response.body?.cancel();
    return failure(
      502,
      `The ${network.displayName} identity backend redirected the search.`,
    );
  }
  const contentType = response.headers.get("content-type") ?? "";
  if (!/^application\/json\b/i.test(contentType)) {
    await response.body?.cancel();
    return failure(
      502,
      `The ${network.displayName} identity backend answered ${response.status} without JSON.`,
    );
  }

  let body: Uint8Array<ArrayBuffer> | null;
  try {
    body = await readBounded(response);
  } catch (error) {
    return unreachable(error, network);
  }
  if (body === null)
    return failure(
      502,
      `The ${network.displayName} identity backend's answer is too large.`,
    );

  const headers = new Headers({
    "content-type": "application/json",
    "cache-control": "no-store",
  });
  const retryAfter = response.headers.get("retry-after");
  if (retryAfter !== null) headers.set("retry-after", retryAfter);
  return new Response(body, { status: response.status, headers });
}
