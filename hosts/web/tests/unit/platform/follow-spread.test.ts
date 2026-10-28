import { describe, expect, test } from "bun:test";
import type { JsonRpcConnection } from "@parity/truapi-host";
import {
  FOLLOWS_PER_CONNECTION,
  spreadFollows,
} from "../../../src/follow-spread.js";

/**
 * A light-client connection that, like the embedded one, refuses a follow
 * beyond the per-connection limit, and records what it is sent.
 */
function fakeLightClient() {
  const connections: FakeConnection[] = [];
  let nextSubscription = 1;
  class FakeConnection implements JsonRpcConnection {
    sent: { method: string; params: unknown[] }[] = [];
    active = new Set<string>();
    closed = false;
    private queue: string[] = [];
    private wake: (() => void) | null = null;
    constructor(readonly name: string) {}
    send(request: string) {
      const { id, method, params = [] } = JSON.parse(request);
      this.sent.push({ method, params });
      if (method === "chainHead_v1_follow") {
        if (this.active.size >= FOLLOWS_PER_CONNECTION)
          return this.reply({
            id,
            error: {
              code: -32800,
              message: "too many active follow subscriptions",
            },
          });
        const subscription = `sub${nextSubscription++}`;
        this.active.add(subscription);
        return this.reply({ id, result: subscription });
      }
      if (method === "chainHead_v1_unfollow") this.active.delete(params[0]);
      this.reply({ id, result: `${method} on ${this.name}` });
    }
    /** The server ends a follow on its own. */
    stop(subscription: string) {
      this.active.delete(subscription);
      this.push({
        method: "chainHead_v1_followEvent",
        params: { subscription, result: { event: "stop" } },
      });
    }
    private reply(message: object) {
      this.push({ jsonrpc: "2.0", ...message });
    }
    private push(message: object) {
      this.queue.push(JSON.stringify(message));
      this.wake?.();
    }
    async *responses() {
      while (!this.closed) {
        const next = this.queue.shift();
        if (next !== undefined) yield next;
        else await new Promise<void>((resolve) => (this.wake = resolve));
      }
    }
    close() {
      this.closed = true;
      this.wake?.();
    }
  }
  const open = async () => {
    const connection = new FakeConnection(`c${connections.length}`);
    connections.push(connection);
    return connection;
  };
  return { connections, open };
}

async function setup() {
  const light = fakeLightClient();
  const spread = spreadFollows(await light.open(), light.open);
  const received: Record<string, unknown>[] = [];
  void (async () => {
    for await (const response of spread.responses())
      received.push(JSON.parse(response));
  })();
  let id = 0;
  const call = async (method: string, params: unknown[] = []) => {
    const requestId = `r${++id}`;
    spread.send(
      JSON.stringify({ jsonrpc: "2.0", id: requestId, method, params }),
    );
    for (let tick = 0; tick < 50; tick += 1) {
      const answer = received.find((message) => message.id === requestId);
      if (answer) return answer;
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
    throw new Error(`no answer to ${method}`);
  };
  return { light, spread, received, call };
}

describe("spreadFollows", () => {
  // Three products following one chain: the third follow would be refused
  // on a single light-client connection.
  test("opens a follow beyond the limit on another connection, and routes its calls there", async () => {
    const { light, call } = await setup();
    const subscriptions: string[] = [];
    for (let follow = 0; follow < 3; follow += 1) {
      const answer = await call("chainHead_v1_follow", [true]);
      expect(answer.error).toBeUndefined();
      subscriptions.push(answer.result as string);
    }
    expect(light.connections.length).toBe(2);
    expect(light.connections.map((c) => c.active.size)).toEqual([2, 1]);

    const third = subscriptions[2]!;
    expect(
      (await call("chainHead_v1_storage", [third, "0xab", []])).result,
    ).toBe("chainHead_v1_storage on c1");
    expect(
      (await call("chainHead_v1_unpin", [subscriptions[0], ["0xab"]])).result,
    ).toBe("chainHead_v1_unpin on c0");
    // Calls that name no follow stay on the first connection.
    expect((await call("chainSpec_v1_genesisHash")).result).toBe(
      "chainSpec_v1_genesisHash on c0",
    );
  });

  test("reuses a slot an unfollow or a server stop frees, and does not count a refused follow", async () => {
    const { light, call } = await setup();
    const first = (await call("chainHead_v1_follow", [true])).result as string;
    const second = (await call("chainHead_v1_follow", [true])).result as string;
    await call("chainHead_v1_unfollow", [first]);
    await call("chainHead_v1_follow", [true]);
    expect(light.connections.length).toBe(1);

    light.connections[0]!.stop(second);
    await new Promise((resolve) => setTimeout(resolve, 0));
    await call("chainHead_v1_follow", [true]);
    expect(light.connections.length).toBe(1);
    expect(light.connections[0]!.active.size).toBe(2);
  });

  test("answers a follow with an error when no further connection can be opened", async () => {
    const light = fakeLightClient();
    const first = await light.open();
    const spread = spreadFollows(first, () =>
      Promise.reject(new Error("too many connections")),
    );
    const received: { id?: string; error?: { message: string } }[] = [];
    void (async () => {
      for await (const response of spread.responses())
        received.push(JSON.parse(response));
    })();
    for (const id of ["a", "b", "c"])
      spread.send(
        JSON.stringify({
          jsonrpc: "2.0",
          id,
          method: "chainHead_v1_follow",
          params: [true],
        }),
      );
    for (let tick = 0; tick < 50 && received.length < 3; tick += 1)
      await new Promise((resolve) => setTimeout(resolve, 0));
    expect(received.find((message) => message.id === "c")?.error?.message).toBe(
      "the light client could not open another connection",
    );
  });

  test("closes every connection it opened", async () => {
    const { light, spread, call } = await setup();
    for (let follow = 0; follow < 3; follow += 1)
      await call("chainHead_v1_follow", [true]);
    spread.close();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(light.connections.map((connection) => connection.closed)).toEqual([
      true,
      true,
    ]);
  });
});
