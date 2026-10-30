// Fetching a descriptor URL: the I/O behind `fetchDescriptor` and the stack reader.
import { wasm } from "./wasm.js";

/** The part of `fetch` that `read` uses. */
export type Fetch = (url: string, init?: RequestInit) => Promise<Response>;

/** How long `read` waits for a descriptor, body included, before treating it as unreachable. */
export const DEFAULT_TIMEOUT_MS = 10_000;

/** What a descriptor URL served: a document and its `ETag`, a `304`, or nothing. */
export type Served = {
  document?: Uint8Array;
  etag?: string;
  notModified: boolean;
};

/**
 * `fetchDescriptor`, sending `ifNoneMatch` as `If-None-Match` so an unchanged descriptor costs a
 * `304` with no body.
 */
export async function fetchServed(
  url: string,
  options: { fetch?: Fetch; timeoutMs?: number; ifNoneMatch?: string } = {},
): Promise<Served> {
  wasm.stackId({ url });
  const controller = new AbortController();
  const timer = setTimeout(
    () => controller.abort(),
    options.timeoutMs ?? DEFAULT_TIMEOUT_MS,
  );
  try {
    const response = await (options.fetch ?? fetch)(url, {
      cache: "no-store",
      redirect: "error",
      signal: controller.signal,
      headers: options.ifNoneMatch
        ? { "if-none-match": options.ifNoneMatch }
        : undefined,
    });
    if (response.status === 304) {
      controller.abort();
      return { notModified: true };
    }
    const document = response.ok ? await readCapped(response) : undefined;
    if (!document) {
      controller.abort();
      return { notModified: false };
    }
    const etag = response.headers.get("etag") ?? undefined;
    return { document, etag, notModified: false };
  } catch (error) {
    const aborted =
      error instanceof DOMException &&
      (error.name === "AbortError" || error.name === "TimeoutError");
    if (!options.fetch || error instanceof TypeError || aborted)
      return { notModified: false };
    throw error;
  } finally {
    clearTimeout(timer);
  }
}

/** The largest descriptor `fetchDescriptor` reads. A descriptor is a few kilobytes. */
export const MAX_DESCRIPTOR_BYTES = 1024 * 1024;

// Reads through `getReader`: Safari before 27 cannot iterate a `ReadableStream` with `for await`.
async function readCapped(response: Response): Promise<Uint8Array | undefined> {
  if (Number(response.headers.get("content-length")) > MAX_DESCRIPTOR_BYTES)
    return undefined;
  if (!response.body) return new Uint8Array(await response.arrayBuffer());
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    length += value.length;
    if (length > MAX_DESCRIPTOR_BYTES) {
      await reader.cancel();
      return undefined;
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return bytes;
}
