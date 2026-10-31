import { describe, expect, test } from "bun:test";
import type { JsonRpcConnection } from "@parity/truapi-host";
import type { Network } from "../../../../src/platform/network.js";
import { noSubmitNetwork } from "../../../../tools/demos/no-submit.js";

/** A light client that answers each request with its method, as it is sent. */
function echoNetwork(sent: string[]): Network {
  return {
    chain: {
      async connect(): Promise<JsonRpcConnection> {
        const answers: string[] = [];
        let wake = () => {};
        return {
          send(request) {
            sent.push(request);
            const { id, method } = JSON.parse(request) as {
              id: number;
              method: string;
            };
            answers.push(JSON.stringify({ id, result: method }));
            wake();
          },
          async *responses() {
            for (;;) {
              while (answers.length > 0) yield answers.shift()!;
              await new Promise<void>((resolve) => (wake = resolve));
            }
          },
          close() {},
        };
      },
    },
  } as unknown as Network;
}

const request = (id: number, method: string) =>
  JSON.stringify({ jsonrpc: "2.0", id, method, params: [] });

/** The next answer on `connection`, read by a reader that then stops. */
async function nextAnswer(connection: JsonRpcConnection): Promise<unknown> {
  for await (const text of connection.responses()) return JSON.parse(text);
  return undefined;
}

describe("the no-submit network", () => {
  // A validate-only tab must not reach the network with a claim or a
  // statement, whoever asks; reads and validation must still work.
  test("refuses every submission before the light client, and passes the rest", async () => {
    const sent: string[] = [];
    const { network, record } = noSubmitNetwork(echoNetwork(sent), () => {});
    const connection = await network.chain.connect(new Uint8Array(32));
    const methods = [
      "state_call",
      "author_submitAndWatchExtrinsic",
      "statement_submit",
      "transaction_v1_broadcast",
      "chainHead_v1_follow",
    ];
    const answers = [];
    for (const [index, method] of methods.entries()) {
      connection.send(request(index, method));
      // Each reader stops after one answer, as a storage read does.
      answers.push(await nextAnswer(connection));
    }

    expect({
      reached: sent.map(
        (text) => (JSON.parse(text) as { method: string }).method,
      ),
      record,
      results: answers.flatMap((answer) =>
        "result" in (answer as object)
          ? [(answer as { result: string }).result]
          : [],
      ),
    }).toEqual({
      reached: ["state_call", "chainHead_v1_follow"],
      record: {
        passed: { state_call: 1, chainHead_v1_follow: 1 },
        refused: [
          "author_submitAndWatchExtrinsic",
          "statement_submit",
          "transaction_v1_broadcast",
        ],
      },
      results: ["state_call", "chainHead_v1_follow"],
    });
  });
});
