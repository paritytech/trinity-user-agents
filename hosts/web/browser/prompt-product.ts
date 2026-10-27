/**
 * A product App for the browser test of the host's prompts: it asks for a
 * contact, or for a device permission, when the test tells it to, and
 * reports the answer as the core returned it. It reaches the host only
 * through TrUAPI.
 */
import { getClientSync } from "@parity/truapi/sandbox";

const found = getClientSync();
if (found === null) throw new Error("no TrUAPI host");
const client = found;

(
  window as unknown as {
    __prompts: {
      pick(): Promise<unknown>;
      camera(): Promise<unknown>;
    };
  }
).__prompts = {
  async pick() {
    const result = await client.contacts.pick({});
    return result.isOk()
      ? { outcome: result.value.outcome }
      : { error: result.error };
  },
  async camera() {
    const result = await client.permissions.requestDevicePermission("Camera");
    return result.isOk()
      ? { granted: result.value.granted }
      : { error: result.error };
  },
};
