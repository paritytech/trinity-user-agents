import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import { ask } from "../../src/app/prompt.js";
import { FieldHost, type FieldSession } from "../../src/fields/host.js";
import type { Network } from "../../src/platform/network.js";
import { printLabel } from "./field-label.js";
import { simulatedStatements } from "./kettle-demo.js";

/**
 * The apps this host lets use the draft field bridge, each by its exact
 * origin and page, and the role it gets. Nothing else may declare fields or
 * ask for them. Each provider is its own product, named by its origin: the
 * same provider app runs on two origins (`npm run dev:field-provider` and
 * `dev:field-provider-2`), and the host keeps their declarations and values
 * apart.
 */
export const FIELD_APPS: readonly {
  origin: string;
  path: string;
  role: "provider" | "consumer";
}[] = [
  { origin: "http://localhost:5182", path: "/provider.html", role: "provider" },
  { origin: "http://localhost:5185", path: "/provider.html", role: "provider" },
  { origin: "http://localhost:5183", path: "/market.html", role: "consumer" },
];

/**
 * DEV ONLY. Wires the draft field bridge (`src/fields/`) into the host.
 *
 * `?field-demo=simulation` makes this tab's People Statement Store the
 * browser-local simulated one and has the core grant allowances without
 * claiming them. Both apply to the whole tab, not only the demo. Never a
 * fallback after a network failure.
 */
export function createFieldDemo(options: {
  runtime(): WorkerPairingHostRuntime;
  log(line: string): void;
}) {
  const simulated =
    new URLSearchParams(location.search).get("field-demo") === "simulation";
  const fields = new FieldHost({
    ask,
    storage: localStorage,
    binder: {
      sign: (product, index, binding) =>
        options.runtime().signFieldKeyBinding(product, index, binding),
      verify: (product, account, binding, signature) =>
        options
          .runtime()
          .verifyFieldKeyBinding(product, account, binding, signature),
    },
    viewUrl: (token, theme) => {
      const url = new URL("./field-view.html", import.meta.url);
      url.searchParams.set("theme", theme);
      url.hash = token;
      return url.href;
    },
    print: printLabel,
  });
  // A host-origin view frame, nested in a product, asks the host for its text.
  window.addEventListener("message", (event) => {
    if (event.origin !== window.location.origin) return;
    const data = event.data as { type?: unknown; token?: unknown } | null;
    if (data?.type !== "web-field-view" || event.ports.length !== 1) return;
    fields.attachView(data.token, event.source, event.ports[0]);
  });
  return {
    setSession: (session: FieldSession | null) => fields.setSession(session),
    /** `network`, with its Statement Store simulated when this tab asked for it. */
    network(network: Network): Network {
      return simulated ? simulatedStatements(network, options.log) : network;
    },
    /** Once the core runs: in simulation, grant allowances without claiming. */
    async started(runtime: WorkerPairingHostRuntime): Promise<void> {
      if (!simulated) return;
      await runtime.setGrantAllowancesUnchecked(true);
      options.log(
        "Field demo simulation: this tab's Statement Store is simulated and allowances are granted without a claim.",
      );
    },
    connect(
      url: URL,
      product: string,
      port: MessagePort,
      frame: () => Window | null,
    ): () => void {
      const role = FIELD_APPS.find(
        (app) => app.origin === url.origin && app.path === url.pathname,
      )?.role;
      if (role === undefined || product !== url.host) {
        port.close();
        return () => {};
      }
      return fields.connect(product, role, port, frame);
    },
  };
}
