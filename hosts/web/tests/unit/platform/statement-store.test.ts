import { describe, expect, test } from "bun:test";
import { MemoryStatementRpc } from "../../support/statement-rpc-memory.js";
import { StatementStore } from "../../../src/platform/statement-store.js";
import type { JsonRpcConnection } from "@parity/truapi-host";

test("eligibility reads the actual submit socket and fails closed for a different or unsupported node", async () => {
  const genesis = `0x${"01".repeat(32)}`;
  const methods = [
    "chainHead_v1_follow",
    "chainHead_v1_storage",
    "chain_getHeader",
    "chain_getBlockHash",
    "chain_getFinalizedHead",
    "state_getStorage",
    "statement_submit",
  ];
  for (const node of [
    { genesis, version: "1.25.1-4a0cc2e7449", methods, checked: true },
    {
      genesis: "0xwrong",
      version: "1.25.1-4a0cc2e7449",
      methods,
      checked: false,
    },
    { genesis, version: "1.24.3-unknown", methods, checked: false },
    { genesis, version: "1.25.1-4a0cc2e7449", methods: [], checked: false },
  ]) {
    const sent: string[] = [];
    let ready!: () => void;
    let closed!: () => void;
    const gate = new Promise<void>((resolve) => {
      ready = resolve;
    });
    const ending = new Promise<void>((resolve) => {
      closed = resolve;
    });
    const connection: JsonRpcConnection = {
      send: (text) => {
        sent.push(text);
        if (sent.length === 3) ready();
      },
      async *responses() {
        await gate;
        for (const [index, result] of [
          node.genesis,
          node.version,
          { methods: node.methods },
        ].entries())
          yield JSON.stringify({ id: index + 1, result });
        await ending;
      },
      close: closed,
    };
    const store = new StatementStore(connection, undefined, genesis);
    expect(await store.supportsBestBlockPosting()).toBe(node.checked);
    expect(sent.map((text) => JSON.parse(text).method)).toEqual([
      "chain_getBlockHash",
      "system_version",
      "rpc_methods",
    ]);
    store.close();
  }
  const unsupported = new StatementStore(
    new MemoryStatementRpc().connect(),
    undefined,
    genesis,
  );
  expect(await unsupported.supportsBestBlockPosting()).toBe(false);
  unsupported.close();
});

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
