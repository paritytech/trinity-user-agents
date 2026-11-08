import { PROBE_PARAM } from "../../src/chat/worker/probe.js";
import type { ChatProductSpec } from "../../src/invitations/wallet-chat.js";
import type { ChatProductKey } from "../chat/products.js";

/**
 * DEV ONLY. sessionStorage key: the chat products whose workers this tab
 * starts with their test probe (`src/chat/worker/probe.ts`), comma
 * separated, such as `contact-chat,kettle-chat`. Absent: none. Only a
 * browser test sets it.
 */
export const WORKER_PROBES_KEY = "truapi-web-signing-host:dev:worker-probes";

/** `spec`, its worker started with its test probe when `storage` asks for one for `key`. */
export function withWorkerProbe(
  key: ChatProductKey,
  spec: ChatProductSpec,
  storage: Pick<Storage, "getItem"> = sessionStorage,
): ChatProductSpec {
  const asked = (storage.getItem(WORKER_PROBES_KEY) ?? "").split(",");
  if (!asked.includes(key)) return spec;
  return {
    ...spec,
    workerParams: { ...spec.workerParams, [PROBE_PARAM]: "1" },
  };
}
