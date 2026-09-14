import type { LocalIdentity } from "./worker-protocol.js";
import type { WorkerSigningHostRuntime } from "./wasm-module.js";
import { errorMessage } from "./error.js";

function stringField(value: unknown, field: string): string {
  if (typeof value !== "object" || value === null || !(field in value)) {
    throw new Error(`identity backend response missing ${field}`);
  }
  const result: unknown = Reflect.get(value, field);
  if (typeof result !== "string" || !result.length) {
    throw new Error(`identity backend response has invalid ${field}`);
  }
  return result;
}

function delay(milliseconds: number, signal: AbortSignal): Promise<void> {
  signal.throwIfAborted();
  const { promise, resolve, reject } = Promise.withResolvers<void>();
  const abort = () => {
    clearTimeout(timer);
    reject(signal.reason);
  };
  const timer = setTimeout(() => {
    signal.removeEventListener("abort", abort);
    resolve();
  }, milliseconds);
  signal.addEventListener("abort", abort, { once: true });
  return promise;
}

/** HTTP stays in the worker; all secret material and proof construction stay native. */
export async function resolveLocalIdentity(
  runtime: WorkerSigningHostRuntime,
  signal: AbortSignal,
  registration?: { baseUsername: string; identityBackendBaseUrl: string },
): Promise<LocalIdentity> {
  const context = runtime.localIdentityContext();
  const check = () => {
    signal.throwIfAborted();
    if (runtime.localIdentityContext().activationId !== context.activationId) {
      throw new Error("local identity activation changed");
    }
  };
  const refresh = async () => {
    check();
    const identity = await runtime.refreshLocalIdentity(context.activationId);
    check();
    if (identity.identityAccountId !== context.identityAccountId) {
      throw new Error(
        "verified identity does not match the active UID account",
      );
    }
    return identity;
  };
  const existing = await refresh();
  if (!registration || existing.liteUsername) return existing;

  const base = registration.identityBackendBaseUrl.replace(/\/+$/, "");
  if (!base) throw new Error("identity backend base URL is empty");
  const request = async (
    path: string,
    init: RequestInit,
  ): Promise<Response> => {
    check();
    const response = await fetch(`${base}${path}`, {
      ...init,
      credentials: "omit",
      redirect: "error",
      signal: AbortSignal.any([signal, AbortSignal.timeout(30_000)]),
    });
    check();
    return response;
  };
  const json = async (path: string, init: RequestInit): Promise<unknown> => {
    const response = await request(path, init);
    const text = await response.text();
    check();
    if (!response.ok) {
      throw new Error(
        `identity backend ${path} failed (${response.status}): ${text}`,
      );
    }
    return JSON.parse(text) as unknown;
  };
  const headers = { "Content-Type": "application/json" };
  const attester = stringField(
    await json("/attester", { method: "GET" }),
    "attester",
  );
  const verifierHex = attester.replace(/^0x/, "");
  if (!/^[0-9a-fA-F]{64}$/.test(verifierHex)) {
    throw new Error("identity backend attester must be 32-byte hex");
  }
  const verifier = Uint8Array.from(verifierHex.match(/../g)!, (byte) =>
    parseInt(byte, 16),
  );
  const challenge = stringField(
    await json("/auth/challenges", { method: "POST", headers, body: "{}" }),
    "challenge",
  );
  const challengeBytes = Uint8Array.from(atob(challenge), (char) =>
    char.charCodeAt(0),
  );
  check();
  const proof = runtime.localIdentityAuthProof(
    context.activationId,
    challengeBytes,
  );
  const token = stringField(
    await json("/auth/token", {
      method: "POST",
      headers: {
        ...headers,
        "Auth-ClientId": btoa(String.fromCharCode(...proof.subarray(0, 32))),
        "Auth-ClientProof": btoa(String.fromCharCode(...proof.subarray(32))),
        "Auth-Challenge": challenge,
      },
      body: "{}",
    }),
    "token",
  );
  check();
  const body = await runtime.localLiteRegistrationBody(
    context.activationId,
    registration.baseUsername,
    verifier,
  );
  check();
  const response = await request("/usernames", {
    method: "POST",
    headers: { ...headers, Authorization: `Bearer ${token}` },
    body,
  });
  const responseText = await response.text();
  check();
  if (!response.ok) {
    throw new Error(
      `username registration failed (${response.status}): ${responseText}`,
    );
  }

  let lastFailure: string | undefined;
  for (let attempt = 0; attempt < 30; attempt++) {
    check();
    try {
      const identity = await refresh();
      if (identity.liteUsername) return identity;
      lastFailure = undefined;
    } catch (error) {
      check();
      lastFailure = errorMessage(error);
    }
    if (attempt < 29) await delay(4_000, signal);
  }
  throw new Error(
    `registration was not confirmed on Asset Hub${lastFailure ? `: ${lastFailure}` : ""}`,
  );
}
