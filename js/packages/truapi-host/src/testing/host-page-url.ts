// The host page reads its configuration from the page URL, so both entry
// points -- the Playwright fixture and a directly-started server -- configure
// a host the same way: by building the same query string.
//
// Keeping it here rather than inline in each means a new option cannot be
// taught to one entry point and forgotten by the other.
import type { MockHostConfig } from "../web/create-mock-host.js";

/** Host configuration the page understands. */
export interface HostPageConfig {
  /** URL of the product under test. */
  productUrl: string;
  /** dotNS identifier the host runs the product under. */
  productId?: string;
  /** Behaviour knobs forwarded to the mock host. */
  mock?: MockHostConfig;
  /** Overrides merged into the host's runtime config. */
  runtimeConfig?: Record<string, unknown>;
  /**
   * Accounts the host can sign as: a built-in name, or a name with the 32
   * bytes of entropy its session activates from.
   *
   * Explicit entropy is how a test signs as an identity the built-ins cannot
   * be -- one enrolled in a personhood ring, for instance, which allowance
   * allocation requires and a fixed dev account never satisfies.
   */
  accounts?: (string | { name: string; entropy: Uint8Array })[];
  /** Whether the host starts signed in. */
  loginBehavior?: "auto" | "manual";
  /**
   * Where the core runs. `"worker"` is the production topology and the
   * default. `"main-thread"` is the debugging one: the core's log output
   * reaches the page console, where `page.on("console")` can read it, instead
   * of the worker console Playwright does not observe.
   */
  topology?: "worker" | "main-thread";
  /**
   * How resource allocation is answered: `"granted"` (the default) without
   * performing it, or `"chain"` for the real path.
   */
  allowances?: "granted" | "chain";
  /**
   * Resource tags answered as refused, whatever `allowances` would otherwise
   * say. Withholding one while the rest stay granted is what makes a product's
   * refusal path reachable.
   */
  withheldResources?: string[];
  /**
   * Core log level (`off`/`error`/`warn`/`info`/`debug`/`trace`). Raising it
   * is what turns a bare failure outcome into the reason behind it: the core
   * logs why a call failed before mapping it to a protocol answer.
   */
  logLevel?: string;
}

/** Apply `config` to a host page URL, returning the configured URL. */
export function hostPageUrl(base: string, config: HostPageConfig): string {
  const url = new URL(base);
  url.searchParams.set("product", config.productUrl);
  if (config.mock) url.searchParams.set("mock", JSON.stringify(config.mock));
  if (config.productId) url.searchParams.set("productId", config.productId);
  if (config.runtimeConfig) {
    url.searchParams.set("runtimeConfig", JSON.stringify(config.runtimeConfig));
  }
  if (config.accounts) {
    // `name` for a built-in, `name:<64 hex>` when the test supplies entropy.
    url.searchParams.set(
      "accounts",
      config.accounts
        .map((account) =>
          typeof account === "string"
            ? account
            : `${account.name}:${[...account.entropy]
                .map((byte) => byte.toString(16).padStart(2, "0"))
                .join("")}`,
        )
        .join(","),
    );
  }
  if (config.loginBehavior) url.searchParams.set("login", config.loginBehavior);
  if (config.topology) url.searchParams.set("topology", config.topology);
  if (config.allowances) url.searchParams.set("allowances", config.allowances);
  if (config.withheldResources?.length) {
    url.searchParams.set("withheld", config.withheldResources.join(","));
  }
  if (config.logLevel) url.searchParams.set("logLevel", config.logLevel);
  return url.toString();
}
