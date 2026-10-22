import { DraftChatSource } from "./draft-chat-source.js";
import type { HostToWorker, WorkerToHost } from "./draft-chat.js";

/** DRAFT, NOT TRUAPI. The Web Worker that runs the synthetic draft chat source. */

const scope = self as unknown as {
  postMessage(message: WorkerToHost): void;
  onmessage: ((event: MessageEvent<HostToWorker>) => void) | null;
};

new DraftChatSource({
  post: (message) => scope.postMessage(message),
  listen: (handler) => {
    scope.onmessage = (event) => handler(event.data);
  },
});
