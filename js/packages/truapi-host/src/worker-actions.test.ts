import { describe, expect, it } from "bun:test";

import {
  CHAT_ACTION_ENTRY_POINT,
  RENDERER_ACTION_ENTRY_POINT,
  handlePublishAction,
  type ActionEntryPoint,
} from "./worker-actions.js";
import type { WorkerProductRuntime } from "./wasm-module.js";
import type { WorkerToMain } from "./worker-protocol.js";

function fakeCore(log: string[]): WorkerProductRuntime {
  return {
    receiveFrame: async () => {},
    dispose: () => {},
    free: () => {},
    publishChatAction: (action) => log.push(`chat:${action.join(",")}`),
    publishRendererAction: (item) => log.push(`renderer:${item.join(",")}`),
    render: () => ({ cancel: () => {}, free: () => {} }),
  };
}

/** Each entry point with the core method it calls and the kind it answers in. */
const entryPoints: {
  entryPoint: ActionEntryPoint;
  responseKind: string;
  logged: string;
  deny: (core: WorkerProductRuntime) => void;
}[] = [
  {
    entryPoint: CHAT_ACTION_ENTRY_POINT,
    responseKind: "publishChatActionResponse",
    logged: "chat:7",
    deny: (core) => {
      core.publishChatAction = () => {
        throw new Error("Denied");
      };
    },
  },
  {
    entryPoint: RENDERER_ACTION_ENTRY_POINT,
    responseKind: "publishRendererActionResponse",
    logged: "renderer:7",
    deny: (core) => {
      core.publishRendererAction = () => {
        throw new Error("Denied");
      };
    },
  },
];

describe("worker action entry points", () => {
  for (const { entryPoint, responseKind, logged, deny } of entryPoints) {
    describe(entryPoint.name, () => {
      it("answers for an unknown core instead of throwing", () => {
        const messages: WorkerToMain[] = [];

        handlePublishAction(
          entryPoint,
          undefined,
          (msg) => messages.push(msg),
          4,
          9,
          new Uint8Array([1]),
        );

        expect(messages).toEqual([
          {
            kind: responseKind,
            requestId: 9,
            ok: false,
            error: `${entryPoint.name} received for unknown core 4`,
          },
        ] as WorkerToMain[]);
      });

      it("reports a core that refuses the action rather than dropping it", () => {
        const messages: WorkerToMain[] = [];
        const core = fakeCore([]);
        deny(core);

        handlePublishAction(
          entryPoint,
          core,
          (msg) => messages.push(msg),
          1,
          3,
          new Uint8Array([7]),
        );

        expect(messages).toEqual([
          { kind: responseKind, requestId: 3, ok: false, error: "Denied" },
        ] as WorkerToMain[]);
      });

      it("hands an accepted action to its own core method", () => {
        const messages: WorkerToMain[] = [];
        const log: string[] = [];

        handlePublishAction(
          entryPoint,
          fakeCore(log),
          (msg) => messages.push(msg),
          1,
          2,
          new Uint8Array([7]),
        );

        expect(log).toEqual([logged]);
        expect(messages).toEqual([
          { kind: responseKind, requestId: 2, ok: true },
        ] as WorkerToMain[]);
      });
    });
  }
});
