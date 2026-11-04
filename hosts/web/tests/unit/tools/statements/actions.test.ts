import { describe, expect, test } from "bun:test";
import type { JsonRpcConnection } from "@parity/truapi-host";
import { bytesToHex } from "@parity/truapi/scale";
import type { StatementTraffic } from "../../../../src/platform/statement-observer.js";
import {
  decodeGrant,
  readGrants,
} from "../../../../tools/statements/actions.js";
import { StatementLedger } from "../../../../tools/statements/ledger.js";
import { peopleTransport } from "../../../../tools/statements/transport.js";
import { evidence } from "../../../../tools/statements/evidence.js";
import { encodeSigned } from "../../../../src/platform/statement.js";

const LIGHT_CLIENT = peopleTransport("light-client");

describe("the Statements view's reads", () => {
  test("decodes a StatementAllowance, and refuses bytes that are not one", () => {
    expect(decodeGrant("0x02000000" + "00d00700")).toEqual({
      maxCount: 2,
      maxSize: 512_000,
    });
    expect(() => decodeGrant("0x0200")).toThrow();
  });

  // In this host's light client every People connection shares one topic
  // filter sent to peers, and a connection's statement subscriptions replace
  // it. The inspector must therefore never subscribe of its own accord.
  test("a grant read opens no statement subscription: it follows the chain, reads storage and closes", async () => {
    const sent: string[] = [];
    let closed = 0;
    const genesis = "0x" + "aa".repeat(32);
    const value = "0x02000000" + "00d00700";
    const connection: JsonRpcConnection = {
      send: (text) => void sent.push(text),
      async *responses() {
        const methods = () => sent.map((text) => JSON.parse(text));
        for (;;) {
          const last = methods().at(-1);
          if (last?.method === "chainHead_v1_follow") {
            yield JSON.stringify({ jsonrpc: "2.0", id: last.id, result: "f" });
            yield JSON.stringify({
              jsonrpc: "2.0",
              method: "chainHead_v1_followEvent",
              params: {
                subscription: "f",
                result: {
                  event: "initialized",
                  finalizedBlockHashes: ["0xb1"],
                },
              },
            });
          }
          if (last?.method === "chainHead_v1_storage") {
            yield JSON.stringify({
              jsonrpc: "2.0",
              id: last.id,
              result: { result: "started", operationId: "o" },
            });
            yield JSON.stringify({
              jsonrpc: "2.0",
              method: "chainHead_v1_followEvent",
              params: {
                subscription: "f",
                result: {
                  event: "operationStorageItems",
                  operationId: "o",
                  items: [{ key: last.params[2][0].key, value }],
                },
              },
            });
            yield JSON.stringify({
              jsonrpc: "2.0",
              method: "chainHead_v1_followEvent",
              params: {
                subscription: "f",
                result: { event: "operationStorageDone", operationId: "o" },
              },
            });
            return;
          }
          await new Promise((resolve) => setTimeout(resolve, 1));
        }
      },
      close: () => void (closed += 1),
    };
    const read = await readGrants(
      {
        chain: { connect: async () => connection },
        genesis: genesis as `0x${string}`,
      },
      ["0x" + "ab".repeat(32)],
    );
    expect(read).toEqual({
      block: "0xb1",
      values: [
        {
          account: "0x" + "ab".repeat(32),
          value: { maxCount: 2, maxSize: 512_000 },
        },
      ],
    });
    const methods = sent.map((text) => JSON.parse(text).method as string);
    expect(methods.some((method) => method.startsWith("statement_"))).toBe(
      false,
    );
    expect(methods).toContain("chainHead_v1_storage");
    expect(closed).toBe(1);
  });

  test("the inspector's tools offer no way to subscribe: no probe is exported", async () => {
    const actions = await import("../../../../tools/statements/actions.js");
    expect(Object.keys(actions).sort()).toEqual(["decodeGrant", "readGrants"]);
  });
});

describe("the Statements view's evidence", () => {
  // A report must say which People backend its facts came from: the same
  // answer means a broadcast over the light client and a stored statement
  // on a node.
  test("names People's backend and describes it", () => {
    for (const backend of ["light-client", "rpc"] as const) {
      const transport = peopleTransport(backend);
      const report = evidence(new StatementLedger(transport), {
        network: "testnet",
        exportedAt: 0,
      });
      expect(report).toMatchObject({
        peopleBackend: backend,
        transport: transport.note,
      });
      expect(report.notes).toEqual(
        expect.arrayContaining(transport.evidenceNotes),
      );
    }
  });

  test("holds metadata only: no statement data even while capture is on", () => {
    const ledger = new StatementLedger(LIGHT_CLIENT);
    ledger.startSession("s", "session");
    ledger.setCapture(true);
    const data = new TextEncoder().encode("a private-looking message");
    const encoded = bytesToHex(
      encodeSigned(
        {
          expiry: 1n << 40n,
          channel: null,
          topics: [new Uint8Array(32).fill(1)],
          data,
        },
        new Uint8Array(32).fill(7),
        () => new Uint8Array(64),
      ),
    );
    ledger.traffic({ type: "connected", connection: 1, source: "core" });
    ledger.traffic({
      type: "request",
      connection: 1,
      id: "1",
      method: "statement_submit",
      params: [encoded],
    });
    ledger.traffic({
      type: "response",
      connection: 1,
      id: "1",
      method: "statement_submit",
      result: { status: "new" },
    });
    expect(ledger.statementList()[0].payload).toBe(bytesToHex(data));
    const text = JSON.stringify(
      evidence(ledger, { network: "testnet", exportedAt: 0 }),
    );
    expect(text).not.toContain(bytesToHex(data).slice(2));
    expect(text).not.toContain(encoded.slice(2));
    expect(text).not.toContain("private-looking");
    expect(text).toContain(ledger.statementList()[0].meta.hash);
  });

  test("holds no statement bytes from answers, pages or errors in shapes it does not know", () => {
    const ledger = new StatementLedger(LIGHT_CLIENT);
    ledger.startSession("s", "session");
    const secret = "0x" + "a7".repeat(200);
    const feed = (event: StatementTraffic) => ledger.traffic(event);
    feed({ type: "connected", connection: 1, source: "core" });
    feed({
      type: "request",
      connection: 1,
      id: "1",
      method: "statement_submit",
      params: [secret],
    });
    feed({
      type: "response",
      connection: 1,
      id: "1",
      method: "statement_submit",
      error: { code: 1, message: secret, data: [secret, { inner: secret }] },
    });
    feed({
      type: "notification",
      connection: 1,
      subscription: "9",
      method: "statement_statement",
      result: { event: "newStatementsV2", data: { items: [secret] } },
    });
    feed({ type: "closed", connection: 1, by: "failed", reason: secret });
    const text = JSON.stringify(
      evidence(ledger, { network: "testnet", exportedAt: 0 }),
    );
    expect(text).not.toContain("a7a7a7a7a7a7a7a7");
    expect(text).toContain("<string, 402 chars>");
    expect(text).toContain("0x…(200 bytes)");
  });
});
