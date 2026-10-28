import { describe, expect, test } from "bun:test";
import {
  NETWORKS,
  type NetworkConfig,
} from "../../../src/platform/network-config.js";
import {
  type Fetcher,
  searchUsernames,
} from "../../../src/contacts/username-search.js";

const PAGE = { usernames: [], nextCursor: null };

/** A backend that records what it was asked and answers with `answer`. */
function backend(answer: () => Response | Promise<Response>) {
  const asked: Request[] = [];
  const fetcher: Fetcher = async (request) => {
    asked.push(request);
    return answer();
  };
  return { asked, fetcher };
}

const json = (value: unknown, init: ResponseInit = {}) =>
  Response.json(value, init);

const search = (query: string, fetcher: Fetcher, method = "GET") =>
  searchUsernames(
    method,
    new URL(`http://localhost:5180/__username-search?${query}`),
    fetcher,
  );

describe("searchUsernames", () => {
  test("asks only the chosen network's backend, with only the search parameters and no page headers", async () => {
    const { asked, fetcher } = backend(() => json(PAGE));
    const answer = await search(
      "network=paseo&prefix=ali.0&limit=5&cursor=eyJh-_x",
      fetcher,
    );

    expect(answer.status).toBe(200);
    expect(asked.map((request) => request.url)).toEqual([
      "https://identity.dotspark.app/api/v1/usernames/search?prefix=ali.0&limit=5&cursor=eyJh-_x",
    ]);
    expect([...asked[0].headers]).toEqual([["accept", "application/json"]]);
    expect(asked[0].redirect).toBe("manual");
  });

  test("a PreviewNet search reaches the PreviewNet backend, never Paseo's", async () => {
    const { asked, fetcher } = backend(() => json(PAGE));
    await search("network=previewnet&prefix=a", fetcher);
    expect(asked.map((request) => request.url)).toEqual([
      "https://identity-previewnet.dotspark.app/api/v1/usernames/search?prefix=a",
    ]);
  });

  test("a value cannot change the backend's host or path", async () => {
    const { asked, fetcher } = backend(() => json(PAGE));
    await search(
      `network=paseo&prefix=${encodeURIComponent("@evil.example/../x?y#z")}`,
      fetcher,
    );
    const reached = new URL(asked[0].url);
    expect([
      reached.origin,
      reached.pathname,
      reached.searchParams.get("prefix"),
    ]).toEqual([
      "https://identity.dotspark.app",
      "/api/v1/usernames/search",
      "@evil.example/../x?y#z",
    ]);
  });

  test("passes the backend's page on as it is", async () => {
    const page = {
      usernames: [
        { accountId: "5Example", username: "alice.06", status: "ASSIGNED" },
      ],
      nextCursor: "next",
    };
    const { fetcher } = backend(() => json(page));
    const answer = await search("network=paseo&prefix=alice", fetcher);
    expect([
      answer.status,
      answer.headers.get("cache-control"),
      await answer.json(),
    ]).toEqual([200, "no-store", page]);
  });

  test("passes the backend's own refusals on, so the page can show why", async () => {
    const invalid = { error: "Invalid cursor" };
    const limited = {
      error: "Rate limit exceeded. Please retry after 60 seconds.",
    };
    const answers = [
      json(invalid, { status: 400 }),
      json(limited, { status: 429, headers: { "retry-after": "60" } }),
    ];
    const { fetcher } = backend(() => answers.shift()!);

    const first = await search("network=paseo&prefix=a&cursor=bad", fetcher);
    const second = await search("network=paseo&prefix=a", fetcher);
    expect([
      [first.status, await first.json()],
      [second.status, second.headers.get("retry-after"), await second.json()],
    ]).toEqual([
      [400, invalid],
      [429, "60", limited],
    ]);
  });

  test.each([
    ["an unknown parameter", "network=paseo&prefix=a&host=evil.example", 400],
    ["a backend-only parameter", "network=paseo&prefix=a&status=ASSIGNED", 400],
    ["a repeated parameter", "network=paseo&prefix=a&prefix=b", 400],
    [
      "an overlong value",
      `network=paseo&prefix=a&cursor=${"x".repeat(2049)}`,
      400,
    ],
    ["no network", "prefix=a", 400],
    ["an unknown network", "network=mainnet&prefix=a", 400],
    ["a network name from the prototype", "network=__proto__&prefix=a", 400],
  ])("refuses %s without asking the backend", async (_, query, status) => {
    const { asked, fetcher } = backend(() => json(PAGE));
    const answer = await search(query, fetcher);
    expect([answer.status, asked.length]).toEqual([status, 0]);
    expect(await answer.json()).toEqual({ error: expect.any(String) });
  });

  test("answers GET only", async () => {
    const { asked, fetcher } = backend(() => json(PAGE));
    const answer = await search("network=paseo&prefix=a", fetcher, "POST");
    expect([answer.status, answer.headers.get("allow"), asked.length]).toEqual([
      405,
      "GET",
      0,
    ]);
  });

  test("says so when the network names no backend", async () => {
    const networks: Record<string, NetworkConfig> = {
      paseo: { ...NETWORKS.paseo, identityBackend: null },
    };
    const { asked, fetcher } = backend(() => json(PAGE));
    const answer = await searchUsernames(
      "GET",
      new URL("http://localhost/__username-search?network=paseo&prefix=a"),
      fetcher,
      networks,
    );
    expect([answer.status, asked.length]).toEqual([503, 0]);
  });

  test.each([
    [
      "a redirect",
      () =>
        json(PAGE, {
          status: 302,
          headers: { location: "https://elsewhere.example/" },
        }),
      502,
    ],
    [
      "an answer that is not JSON",
      () =>
        new Response("<html>", {
          status: 502,
          headers: { "content-type": "text/html" },
        }),
      502,
    ],
    [
      "an answer too large",
      () =>
        new Response(new Uint8Array(1024 * 1024 + 1), {
          headers: { "content-type": "application/json" },
        }),
      502,
    ],
    [
      "an unreachable backend",
      () => Promise.reject(new TypeError("fetch failed")),
      502,
    ],
    [
      "a timeout",
      () => Promise.reject(new DOMException("timed out", "TimeoutError")),
      504,
    ],
  ])(
    "reports %s as an error, never as an empty page",
    async (_, answer, status) => {
      const { fetcher } = backend(answer);
      const response = await search("network=paseo&prefix=a", fetcher);
      const body = await response.json();
      expect([response.status, typeof body.error, "usernames" in body]).toEqual(
        [status, "string", false],
      );
    },
  );
});
