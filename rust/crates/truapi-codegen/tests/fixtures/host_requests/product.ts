import { createInterface } from "node:readline";
import { err, ok } from "neverthrow";
import { createClient } from "./generated/client.js";
import { WIRE_DECODE_TABLE } from "./generated/wire-decode.js";
import { createTransport } from "./runtime.js";
import {
  decodeWireMessage,
  MESSAGE_TYPE_INTERRUPT,
  MESSAGE_TYPE_RECEIVE,
  MESSAGE_TYPE_STOP,
  type WireProvider,
} from "./transport.js";

function decodeFrame(frame: Uint8Array) {
  const message = decodeWireMessage(frame);
  if (message.isErr()) throw message.error;
  return message.value;
}

let receive: (frame: Uint8Array) => void = () => {
  throw new Error("transport did not subscribe");
};
let complete: () => void = () => {};
const responses: Uint8Array[] = [];
const deadline = setTimeout(() => {
  console.error("host exchange timed out");
  process.exit(1);
}, 5_000);
const answered = new Promise<void>((resolve) => {
  complete = resolve;
});
const provider: WireProvider = {
  postMessage(frame) {
    responses.push(frame);
    process.stdout.write(Buffer.from(frame).toString("hex") + "\n");
    complete();
  },
  subscribe(listener) {
    receive = listener;
    return () => {};
  },
  dispose() {},
};
const transport = createTransport(provider);
const client = createClient(transport);
client.probe.onAnswer(async ({ text, outcome }, { signal }) => {
  if (signal.aborted) throw new Error("request arrived cancelled");
  switch (outcome) {
    case 0:
      return ok({ text: text.toUpperCase() });
    case 1:
      return err({
        tag: "Domain",
        value: {
          tag: "V2",
          value: { tag: "Rejected", value: { reason: text } },
        },
      });
    case 2:
      throw new Error(text);
    default:
      return err({ tag: "Denied" });
  }
});
try {
  const requestFrame = new Uint8Array(Buffer.from(process.argv[2]!, "hex"));
  const request = decodeFrame(requestFrame);
  receive(requestFrame);
  await answered;
  if (responses.length !== 1) throw new Error("the call answered twice");
  const { payload } = decodeFrame(responses[0]!);
  const decode =
    WIRE_DECODE_TABLE[payload.traitId * 256 + payload.methodId]?.[
      payload.messageType
    ];
  if (!decode) throw new Error("the debugger has no decoder for the response");
  decode(payload.value);
  if (payload.messageType === MESSAGE_TYPE_RECEIVE) {
    const input = createInterface({ input: process.stdin });
    try {
      const line = await input[Symbol.asyncIterator]().next();
      if (line.done) throw new Error("the host did not send Stop");
      const frame = new Uint8Array(Buffer.from(line.value, "hex"));
      const stop = decodeFrame(frame);
      if (
        stop.requestId !== request.requestId ||
        stop.payload.traitId !== request.payload.traitId ||
        stop.payload.methodId !== request.payload.methodId ||
        stop.payload.messageType !== MESSAGE_TYPE_STOP ||
        stop.payload.value.length !== 0
      ) {
        throw new Error("the host sent an invalid Stop");
      }
      receive(frame);
      await Promise.resolve();
      if (responses.length !== 1)
        throw new Error("Stop caused another response");
    } finally {
      input.close();
    }
  } else if (payload.messageType !== MESSAGE_TYPE_INTERRUPT) {
    throw new Error("the call did not use subscription framing");
  }
} finally {
  clearTimeout(deadline);
  transport.dispose();
}
