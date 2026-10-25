import type { Link, RpcEndpoint } from "./config";

export type CheckResult = {
  name: string;
  url: string;
  ok: boolean;
  detail: string;
};

type Fetch = typeof fetch;

const TIMEOUT_MS = 20_000;

async function rpcCall(
  fetchFn: Fetch,
  url: string,
  method: string,
): Promise<unknown> {
  const response = await fetchFn(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ id: 1, jsonrpc: "2.0", method, params: [] }),
    signal: AbortSignal.timeout(TIMEOUT_MS),
  });
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }
  const body = (await response.json()) as {
    result?: unknown;
    error?: { message?: string };
  };
  if (body.error) {
    throw new Error(body.error.message ?? "RPC error");
  }
  return body.result;
}

export async function checkRpc(
  endpoint: RpcEndpoint,
  fetchFn: Fetch = fetch,
): Promise<CheckResult & { block?: number; peers?: number }> {
  const base = { name: endpoint.name, url: endpoint.url };
  try {
    const [health, header] = await Promise.all([
      rpcCall(fetchFn, endpoint.url, "system_health") as Promise<{
        peers: number;
        isSyncing: boolean;
      }>,
      rpcCall(fetchFn, endpoint.url, "chain_getHeader") as Promise<{
        number: string;
      }>,
    ]);
    const block = Number.parseInt(header.number, 16);
    if (health.peers < endpoint.minPeers) {
      return {
        ...base,
        ok: false,
        block,
        peers: health.peers,
        detail: `${health.peers} peers, needs ${endpoint.minPeers}`,
      };
    }
    if (health.isSyncing) {
      return {
        ...base,
        ok: false,
        block,
        peers: health.peers,
        detail: "syncing",
      };
    }
    return {
      ...base,
      ok: true,
      block,
      peers: health.peers,
      detail: `block ${block}`,
    };
  } catch (error) {
    return { ...base, ok: false, detail: describe(error) };
  }
}

export async function checkLink(
  link: Link,
  fetchFn: Fetch = fetch,
): Promise<CheckResult> {
  const base = { name: link.name, url: link.url };
  try {
    const response = await fetchFn(link.url, {
      method: link.method,
      redirect: "follow",
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
    if (!response.ok) {
      return { ...base, ok: false, detail: `HTTP ${response.status}` };
    }
    if (link.lookFor && !(await response.text()).includes(link.lookFor)) {
      return { ...base, ok: false, detail: `"${link.lookFor}" not found` };
    }
    return { ...base, ok: true, detail: `HTTP ${response.status}` };
  } catch (error) {
    return { ...base, ok: false, detail: describe(error) };
  }
}

function describe(error: unknown): string {
  if (error instanceof Error) {
    return error.name === "TimeoutError" ? "timed out" : error.message;
  }
  return String(error);
}
