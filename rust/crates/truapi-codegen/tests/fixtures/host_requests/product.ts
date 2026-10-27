import { err, ok } from "neverthrow";
import { createClient } from "./generated/client.js";
import { createTransport } from "./runtime.js";
import type { WireProvider } from "./transport.js";

let receive: (frame: Uint8Array) => void = () => {
  throw new Error("transport did not subscribe");
};
let complete: () => void = () => {};
let responses = 0;
let responseDeadline: ReturnType<typeof setTimeout> | undefined;
const answered = new Promise<void>((resolve, reject) => {
  complete = resolve;
  responseDeadline = setTimeout(
    () => reject(new Error("the call did not answer")),
    5_000,
  );
});
const provider: WireProvider = {
  postMessage(frame) {
    responses++;
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
  receive(new Uint8Array(Buffer.from(process.argv[2]!, "hex")));
  await answered;
  if (responses !== 1) throw new Error("the call answered twice");
} finally {
  clearTimeout(responseDeadline);
  transport.dispose();
}
