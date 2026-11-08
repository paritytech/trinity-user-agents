import { describe, expect, test } from "bun:test";
import { CONTACT_CHAT } from "../../../../src/invitations/wallet-chat.js";
import {
  WORKER_PROBES_KEY,
  withWorkerProbe,
} from "../../../../tools/demos/worker-probes.js";
import { MemoryStorage } from "../../../support/test-support.js";

/**
 * A chat worker starts with its test probe only when the tab asks for one
 * for that product, and keeps every other setting its host passes it.
 */
describe("a chat worker's test probe", () => {
  test("is off unless the tab asks for it, and then only for the products it names", () => {
    const storage = new MemoryStorage();
    const kettle = { ...CONTACT_CHAT, workerParams: { iroh: "public" } };
    const params = () => ({
      contact: withWorkerProbe("contact-chat", CONTACT_CHAT, storage)
        .workerParams,
      kettle: withWorkerProbe("kettle-chat", kettle, storage).workerParams,
    });
    const off = params();
    storage.setItem(WORKER_PROBES_KEY, "kettle-chat");
    expect([off, params()]).toEqual([
      { contact: undefined, kettle: { iroh: "public" } },
      { contact: undefined, kettle: { iroh: "public", probe: "1" } },
    ]);
  });
});
