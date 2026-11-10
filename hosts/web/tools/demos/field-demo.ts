import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { StatementStoreLike } from "../../src/platform/statement-store.js";
import { StatementStore } from "../../src/platform/statement-store.js";
import type { AllowanceGuard } from "../../src/platform/allowance-usable.js";
import { ask } from "../../src/app/prompt.js";
import { FieldHost } from "../../src/fields/host.js";
import { FieldError } from "../../bridges/fields/protocol.js";
import { printLabel } from "./field-label.js";
import { FIELD_INDEX, openFieldTransport } from "../../src/fields/transport.js";
import { LocalStatementNode } from "../../fixtures/statements/local-statement-node.js";
import { pageStore } from "../../fixtures/invitations/shared-store.js";

/**
 * The apps this host lets use the field bridge, each by its exact origin and
 * page, and the role it gets. Nothing else may declare fields or ask for an
 * address. Each provider is its own product, named by its origin: the same
 * provider app runs on two origins (`npm run dev:field-provider` and
 * `dev:field-provider-2`), and the host keeps their declarations and values
 * apart.
 */
export const FIELD_APPS: readonly {
  origin: string;
  path: string;
  role: "provider" | "market";
}[] = [
  { origin: "http://localhost:5182", path: "/provider.html", role: "provider" },
  { origin: "http://localhost:5185", path: "/provider.html", role: "provider" },
  { origin: "http://localhost:5183", path: "/market.html", role: "market" },
];

/** Dev-only wiring. Simulation is explicit, never a fallback after a network failure. */
export function createFieldDemo(options: {
  runtime(): WorkerPairingHostRuntime;
  store(): Promise<StatementStoreLike>;
  usable: AllowanceGuard;
  network: string;
}) {
  const simulated =
    new URLSearchParams(location.search).get("field-demo") === "simulation";
  const node = simulated
    ? new LocalStatementNode({
        store: pageStore(),
        network: "field-demo-simulation",
      })
    : null;
  const fields = new FieldHost({
    ask,
    viewUrl: (token, theme) => {
      const url = new URL("./field-view.html", import.meta.url);
      url.searchParams.set("theme", theme);
      url.hash = token;
      return url.href;
    },
    print: printLabel,
    open: (product, receive, signal) =>
      openFieldTransport({
        product,
        receive,
        runtime: options.runtime(),
        signal,
        network: {
          kind: simulated ? "simulated" : "live",
          name: options.network,
        },
        store:
          node === null
            ? options.store
            : async () =>
                new StatementStore(
                  node.connect(() =>
                    Promise.reject(
                      new Error("Simulation has no network fallback."),
                    ),
                  ),
                ),
        allow: async (signal) => {
          signal.throwIfAborted();
          if (simulated) return;
          try {
            const claim = await options
              .runtime()
              .claimProductStatementAccount(product, FIELD_INDEX, true);
            signal.throwIfAborted();
            const usable = await options.usable(claim, {
              wait: "finalized",
              signal,
            });
            if (usable.grant.maxCount < 2 || usable.grant.maxSize < 4096)
              throw new Error("Insufficient allowance.");
          } catch (error) {
            signal.throwIfAborted();
            const reason = error instanceof Error ? error.message : "unknown";
            throw new FieldError(
              "network",
              `Test-network allowance unavailable (${reason}). Use a disposable registered account with a free Statement Store slot. No mock fallback was used.`,
            );
          }
        },
      }),
  });
  // A host-origin view frame, nested in a product, asks the host for its text.
  window.addEventListener("message", (event) => {
    if (event.origin !== window.location.origin) return;
    const data = event.data as { type?: unknown; token?: unknown } | null;
    if (data?.type !== "web-field-view" || event.ports.length !== 1) return;
    fields.attachView(data.token, event.source, event.ports[0]);
  });
  return {
    setScope: (scope: string | null) => fields.setScope(scope),
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
