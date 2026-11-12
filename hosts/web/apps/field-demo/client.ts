import "./style.css";
import { connectLocket } from "../../bridges/locket/product.js";
import { LocketError, type LocketRole } from "../../bridges/locket/protocol.js";

export { LocketError };

/** Every message the host sent over the Locket bridge, for this demo's privacy checks. */
declare global {
  interface Window {
    __fieldDemoReceived: unknown[];
  }
}

/** Connect this page's Locket bridge in `role`, recording what the host sends. */
export function connect<Role extends LocketRole>(role: Role) {
  window.__fieldDemoReceived = [];
  return connectLocket(role, {
    observe: (message) => {
      window.__fieldDemoReceived.push(message);
      if (window.__fieldDemoReceived.length > 100)
        window.__fieldDemoReceived.shift();
    },
  });
}

/** A required page element. */
export function element<T extends HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (found === null) throw new Error("Missing page element.");
  return found as T;
}
