/**
 * A product App for the browser test of open apps: it asks the host to open
 * another product with TrUAPI's `navigateTo` when the test tells it to, and
 * keeps a value of its own so the test can tell its document survived. It
 * reaches the host only through TrUAPI.
 */
import { getClientSync } from "@parity/truapi/sandbox";

const found = getClientSync();
if (found === null) throw new Error("no TrUAPI host");
const client = found;

(
  window as unknown as {
    __launcher: {
      state: number;
      navigate(url: string): Promise<unknown>;
    };
  }
).__launcher = {
  state: Math.random(),
  async navigate(url) {
    const result = await client.system.navigateTo({ url });
    return result.isOk() ? { ok: true } : { error: result.error };
  },
};
