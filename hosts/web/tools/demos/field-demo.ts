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

/** Where the dev server serves field images (`field-image-plugin.ts`). */
const IMAGES = `${import.meta.env.BASE_URL}__field-images/`;

/** How long a call to the image service may take before it fails. */
const IMAGES_TIMEOUT_MS = 10_000;

/** A same-origin call to the dev server's field images, failing after {@link IMAGES_TIMEOUT_MS}. */
const imagesPost = (path: string, body?: unknown): Promise<Response> =>
  fetch(`${IMAGES}${path}`, {
    method: "POST",
    headers: { "x-field-images": "1", "content-type": "application/json" },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    credentials: "same-origin",
    cache: "no-store",
    signal: AbortSignal.timeout(IMAGES_TIMEOUT_MS),
  });

/**
 * The web host's local image service: the dev server's single-use images
 * (`field-image-plugin.ts`), each for the product origin `FIELD_APPS` gives
 * the product id.
 */
function fieldImages() {
  return {
    async issue(image: {
      product: string;
      svg: string;
    }): Promise<{ url: string; expires: number; revoke(): Promise<void> }> {
      const origin = FIELD_APPS.find(
        (app) => new URL(app.origin).host === image.product,
      )?.origin;
      if (origin === undefined) throw new Error("No origin for this product.");
      const answer = await imagesPost("issue", { origin, svg: image.svg });
      if (!answer.ok) throw new Error("The dev server issued no image.");
      const { token, ttl } = (await answer.json()) as {
        token: string;
        ttl: number;
      };
      return {
        url: new URL(`${IMAGES}${token}.svg`, location.origin).href,
        expires: Date.now() + ttl,
        revoke: () =>
          imagesPost("revoke", { tokens: [token] }).then(
            () => {},
            () => {},
          ),
      };
    },
  };
}

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
    images: fieldImages(),
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
    print: printLabel,
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
    connect(url: URL, product: string, port: MessagePort): () => void {
      const role = FIELD_APPS.find(
        (app) => app.origin === url.origin && app.path === url.pathname,
      )?.role;
      if (role === undefined || product !== url.host) {
        port.close();
        return () => {};
      }
      return fields.connect(product, role, port);
    },
  };
}
