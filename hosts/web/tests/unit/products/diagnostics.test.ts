import { describe, expect, test } from "bun:test";
import {
  DIAGNOSTICS_LIMITS,
  DIAGNOSTICS_PROTOCOL,
  PRODUCT_DIAGNOSTICS,
  readDiagnosticsMessage,
} from "../../../src/products/diagnostics.js";

const ACCOUNT = "0x" + "ab".repeat(32);
const TOPIC = "0x" + "11".repeat(32);

const message = (report: unknown) => ({
  type: PRODUCT_DIAGNOSTICS,
  protocol: DIAGNOSTICS_PROTOCOL,
  report,
});

describe("product diagnostics reports", () => {
  test("keeps the fields that read, and leaves out the ones that do not", () => {
    expect(
      readDiagnosticsMessage(
        message({
          conversations: [
            {
              id: "c1",
              protocol: "chat/simple/1",
              state: "connected",
              request: "0x" + "22".repeat(16),
              account: ACCOUNT.toUpperCase().replace("0X", "0x"),
              peers: [ACCOUNT, "not hex", 7],
              listen: [TOPIC],
              post: { topics: [TOPIC], channel: "0x12" },
              logical: { sent: 3, received: -1, posts: 1.5, failed: 0 },
              window: { messages: 2, bytes: 100, maxMessages: 20 },
              facts: [
                { label: "Mode", value: "plain" },
                { label: "Rounds", value: 4 },
                { label: "", value: "dropped" },
              ],
              secret: "anything else is dropped",
            },
            { id: "no protocol" },
          ],
        }),
      ),
    ).toEqual({
      conversations: [
        {
          id: "c1",
          protocol: "chat/simple/1",
          state: "connected",
          request: "0x" + "22".repeat(16),
          account: ACCOUNT,
          peers: [ACCOUNT],
          listen: [TOPIC],
          post: { topics: [TOPIC] },
          logical: { sent: 3, failed: 0 },
          window: { messages: 2, bytes: 100, maxMessages: 20 },
          facts: [
            { label: "Mode", value: "plain" },
            { label: "Rounds", value: "4" },
          ],
        },
      ],
    });
  });

  test("bounds every list and text, and refuses what is not a report", () => {
    const report = readDiagnosticsMessage(
      message({
        conversations: Array.from({ length: 50 }, (_, index) => ({
          id: `c${index}`,
          protocol: "p".repeat(500),
          listen: Array.from({ length: 40 }, () => TOPIC),
        })),
      }),
    )!;
    expect(report.conversations).toHaveLength(DIAGNOSTICS_LIMITS.conversations);
    expect(report.conversations[0].protocol).toHaveLength(
      DIAGNOSTICS_LIMITS.text,
    );
    expect(report.conversations[0].listen).toHaveLength(
      DIAGNOSTICS_LIMITS.topics,
    );
    expect(readDiagnosticsMessage({ type: PRODUCT_DIAGNOSTICS })).toBeNull();
    expect(
      readDiagnosticsMessage({
        ...message({ conversations: [] }),
        protocol: "v9",
      }),
    ).toBeNull();
    expect(readDiagnosticsMessage(null)).toBeNull();
  });
});
