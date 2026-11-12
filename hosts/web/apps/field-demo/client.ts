import "./style.css";
import { connectFields } from "../../bridges/fields/product.js";
import { FieldError, type FieldRole } from "../../bridges/fields/protocol.js";

export { FieldError };

/** Every message the host sent over the field bridge, for this demo's privacy checks. */
declare global {
  interface Window {
    __fieldDemoReceived: unknown[];
  }
}

/** Connect this page's field bridge in `role`, recording what the host sends. */
export function connect<Role extends FieldRole>(role: Role) {
  window.__fieldDemoReceived = [];
  return connectFields(role, {
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
