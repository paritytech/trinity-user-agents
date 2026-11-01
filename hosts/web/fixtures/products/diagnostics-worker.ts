/**
 * A product worker for the browser test of the inspector's Statements view.
 * It posts one diagnostics report (`src/products/diagnostics.ts`) about a
 * made-up conversation, as a chat product would, and posts the report again
 * when `window.__diagnosticsWorker.report(state)` is called with a new state.
 */
import { postDiagnostics } from "../../src/products/diagnostics.js";

const report = (state: string) =>
  postDiagnostics({
    conversations: [
      {
        id: "demo-conversation",
        protocol: "demo/plain/1",
        state,
        account: "0x" + "a1".repeat(32),
        listen: ["0x" + "22".repeat(32)],
        post: {
          topics: ["0x" + "11".repeat(32)],
          channel: "0x" + "33".repeat(32),
        },
        logical: { sent: 1, received: 0 },
      },
    ],
  });

report("waiting");
(window as unknown as { __diagnosticsWorker: unknown }).__diagnosticsWorker = {
  report,
};
