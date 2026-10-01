import { describe, expect, test } from "bun:test";
import { TRUAPI_CODEC_VERSION } from "@parity/truapi";
import type { InAppDebugger } from "@parity/truapi-debugger";
import { createFrameTap } from "./frame-tap.js";

interface Fed {
  channelId: string;
  dir: string;
  frame: Uint8Array;
  identity: unknown;
}

function recordingInspector(): { inspector: InAppDebugger; fed: Fed[] } {
  const fed: Fed[] = [];
  const inspector = {
    handleFrame(
      channelId: string,
      dir: string,
      frame: Uint8Array,
      identity: unknown,
    ) {
      fed.push({ channelId, dir, frame, identity });
    },
  } as unknown as InAppDebugger;
  return { inspector, fed };
}

describe("createFrameTap", () => {
  // Decode is gated on the schema of the core that encoded the frame, not the
  // page bundle's own constant, so the tap must forward the core's hash.
  test("stamps frames with the core's wire schema", () => {
    const { inspector, fed } = recordingInspector();
    createFrameTap(
      inspector,
      "myapp.paseo",
      "core-hash",
    )("out", Uint8Array.of(1));
    expect(fed).toEqual([
      {
        channelId: "myapp.paseo",
        dir: "out",
        frame: Uint8Array.of(1),
        identity: { codec: TRUAPI_CODEC_VERSION, schema: "core-hash" },
      },
    ]);
  });

  // Worker transfer can detach the relayed bytes after the tap has seen them.
  test("keeps its own copy of the frame", () => {
    const { inspector, fed } = recordingInspector();
    const frame = Uint8Array.of(7, 8);
    createFrameTap(inspector, "myapp.paseo", undefined)("in", frame);
    frame.fill(0);
    expect(fed[0].frame).toEqual(Uint8Array.of(7, 8));
  });
});
