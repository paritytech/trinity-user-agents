import { describe, expect, test } from "bun:test";
import { MemoryStatementRpc } from "../../../../fixtures/nova/memory-rpc.js";
import { StatementStore } from "../../../../src/chat/nova/statement-store.js";

describe("statement store tap", () => {
  test("sees each JSON-RPC text exactly as it is sent and received", async () => {
    const rpc = new MemoryStatementRpc();
    const connection = rpc.connect();
    const sent: string[] = [];
    const received: string[] = [];
    const send = connection.send.bind(connection);
    connection.send = (request) => {
      sent.push(request);
      send(request);
    };
    const responses = connection.responses.bind(connection);
    connection.responses = () => ({
      async *[Symbol.asyncIterator]() {
        for await (const text of responses()) {
          received.push(text);
          yield text;
        }
      },
    });
    const tapped: [string, string][] = [];
    const store = new StatementStore(connection, (direction, text) =>
      tapped.push([direction, text]),
    );
    await store.subscribe({ matchAll: [new Uint8Array(32).fill(1)] }, () => {});
    store.close();
    expect(sent).toHaveLength(1);
    expect(received).toHaveLength(1);
    expect(tapped).toEqual([
      ["out", sent[0]],
      ["in", received[0]],
    ]);
  });
});
