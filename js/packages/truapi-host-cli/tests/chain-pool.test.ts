// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import { describe, it, expect } from "bun:test";
import { createChainPool, type SocketLike } from "../src/chain-pool.js";

const GENESIS =
  "0xc5af1826b31493f08b7e2a823842f98575b806a784126f28da9608c68665afa5";

class FakeSocket implements SocketLike {
  listeners = new Map<string, ((event: unknown) => void)[]>();
  sent: string[] = [];
  closed = false;

  addEventListener(type: string, listener: (event: unknown) => void): void {
    const existing = this.listeners.get(type) ?? [];
    this.listeners.set(type, [...existing, listener]);
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.closed = true;
  }

  emit(type: string, event: unknown = {}): void {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(event);
    }
  }

  receive(message: unknown): void {
    this.emit("message", { data: JSON.stringify(message) });
  }
}

function pool(maxLeasesPerSocket = 2) {
  const sockets: FakeSocket[] = [];
  const chainPool = createChainPool({
    endpoints: { [GENESIS]: { rpc: "wss://example.test", name: "people" } },
    maxLeasesPerSocket,
    createSocket: () => {
      const socket = new FakeSocket();
      sockets.push(socket);
      return socket;
    },
  });
  return { chainPool, sockets };
}

/** Collect everything a connection's responses() stream has delivered. */
function collect(connection: { responses(): AsyncIterable<string> }) {
  const received: unknown[] = [];
  void (async () => {
    for await (const json of connection.responses()) {
      received.push(JSON.parse(json));
    }
  })();
  return received;
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("createChainPool", () => {
  it("As the wasm core, I connect to a chain with no configured endpoint and the connection is rejected", async () => {
    // Given
    const { chainPool } = pool();

    // Then
    await expect(chainPool.connect("0xdeadbeef")).rejects.toThrow(
      /no RPC endpoint/,
    );
  });

  it("As the wasm core, I send on two leases of one shared socket and each receives exactly its own response", async () => {
    // Given
    const { chainPool, sockets } = pool();
    const a = await chainPool.connect(GENESIS);
    const b = await chainPool.connect(GENESIS);
    expect(sockets).toHaveLength(1);
    const socket = sockets[0];
    socket.emit("open");

    // When
    // Both leases use the same request id. The pool must keep them apart.
    a.send(JSON.stringify({ jsonrpc: "2.0", id: "p:1", method: "m_a" }));
    b.send(JSON.stringify({ jsonrpc: "2.0", id: "p:1", method: "m_b" }));
    const [wireA, wireB] = socket.sent.map(
      (json) => JSON.parse(json) as { id: string },
    );
    expect(wireA.id).not.toBe(wireB.id);

    const seenA = collect(a);
    const seenB = collect(b);
    socket.receive({ jsonrpc: "2.0", id: wireB.id, result: "for-b" });
    socket.receive({ jsonrpc: "2.0", id: wireA.id, result: "for-a" });
    await tick();

    // Then
    // Each lease got exactly its own response, with the ORIGINAL id restored.
    expect(seenA).toEqual([{ jsonrpc: "2.0", id: "p:1", result: "for-a" }]);
    expect(seenB).toEqual([{ jsonrpc: "2.0", id: "p:1", result: "for-b" }]);
  });

  it("As the wasm core, I subscribe on one lease and its notifications reach only that lease", async () => {
    // Given
    const { chainPool, sockets } = pool();
    const a = await chainPool.connect(GENESIS);
    const b = await chainPool.connect(GENESIS);
    const socket = sockets[0];
    socket.emit("open");

    // When
    a.send(
      JSON.stringify({ jsonrpc: "2.0", id: 1, method: "chainHead_v1_follow" }),
    );
    const wireId = (JSON.parse(socket.sent[0]) as { id: string }).id;
    const seenA = collect(a);
    const seenB = collect(b);
    // The subscribe response names the token (a string result)...
    socket.receive({ jsonrpc: "2.0", id: wireId, result: "sub-token" });
    // ...and notifications for it must reach ONLY the subscribing lease.
    socket.receive({
      jsonrpc: "2.0",
      method: "chainHead_v1_followEvent",
      params: { subscription: "sub-token", result: { event: "initialized" } },
    });
    await tick();

    // Then
    expect(seenA).toHaveLength(2);
    expect(seenB).toHaveLength(0);
  });

  it("As the wasm core, I connect past the lease cap and the pool opens another socket", async () => {
    // Given
    const { chainPool, sockets } = pool(2);
    await chainPool.connect(GENESIS);
    await chainPool.connect(GENESIS);

    // When
    await chainPool.connect(GENESIS);

    // Then
    expect(sockets).toHaveLength(2);
    expect(chainPool.socketCounts()).toEqual({ "wss://example.test": 2 });
  });

  it("As the wasm core, I send before the socket opens and the message flushes on open", async () => {
    // Given
    const { chainPool, sockets } = pool();
    const lease = await chainPool.connect(GENESIS);
    lease.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "early" }));
    expect(sockets[0].sent).toHaveLength(0);

    // When
    sockets[0].emit("open");

    // Then
    expect(sockets[0].sent).toHaveLength(1);
  });

  it("As a host embedder, a freed lease slot is reused and the socket stays open while any lease remains", async () => {
    // Given
    const { chainPool, sockets } = pool(2);
    const a = await chainPool.connect(GENESIS);
    const b = await chainPool.connect(GENESIS);

    // When
    a.close();

    // Then
    expect(sockets[0].closed).toBe(false);

    // The freed slot is reused instead of opening socket #2.
    await chainPool.connect(GENESIS);
    expect(sockets).toHaveLength(1);

    b.close();
    // One lease (the reused slot) still holds it open.
    expect(sockets[0].closed).toBe(false);
  });

  it("As the wasm core, my lease's response stream ends when the socket closes", async () => {
    // Given
    const { chainPool, sockets } = pool();
    const lease = await chainPool.connect(GENESIS);
    const iterator = lease.responses()[Symbol.asyncIterator]();
    const first = iterator.next();

    // When
    sockets[0].emit("close");

    // Then
    expect((await first).done).toBe(true);
    expect(chainPool.socketCounts()).toEqual({});
  });

  it("As the wasm core, responses for a closed lease are dropped instead of leaking to other leases", async () => {
    // Given
    const { chainPool, sockets } = pool();
    const a = await chainPool.connect(GENESIS);
    const b = await chainPool.connect(GENESIS);
    const socket = sockets[0];
    socket.emit("open");
    a.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "m" }));
    const wireId = (JSON.parse(socket.sent[0]) as { id: string }).id;
    const seenB = collect(b);

    // When
    a.close();
    socket.receive({ jsonrpc: "2.0", id: wireId, result: "late" });
    await tick();

    // Then
    expect(seenB).toHaveLength(0);
  });
});
