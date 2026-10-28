/**
 * A product App for the browser test of Contacts: it asks the host's picker
 * for a contact when the test tells it to, and reports the outcome as the
 * core returned it. It reaches the host only through TrUAPI.
 */
import { getClientSync } from "@parity/truapi/sandbox";

const found = getClientSync();
if (found === null) throw new Error("no TrUAPI host");
const client = found;

(window as unknown as { __contacts: { pick(): Promise<unknown> } }).__contacts =
  {
    async pick() {
      const result = await client.contacts.pick({});
      return result.isOk()
        ? { outcome: result.value.outcome }
        : { error: result.error };
    },
  };
