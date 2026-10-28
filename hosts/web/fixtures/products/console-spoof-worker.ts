/**
 * A second product worker for the browser test of the Workers inspector. It
 * posts a console line that claims to come from another worker, which the
 * host must still put down to this one, and opens one TrUAPI subscription,
 * so the TrUAPI view has a second channel.
 */
import { getClientSync } from "@parity/truapi/sandbox";

window.parent.postMessage(
  {
    type: "truapi-worker-console",
    level: "log",
    source: "console",
    text: "spoof from b",
    name: "a.paseo",
    productId: "a.paseo",
  },
  location.origin,
);
console.log("b is up");
getClientSync()
  ?.chat.actionSubscribe()
  .subscribe({ next() {} });
