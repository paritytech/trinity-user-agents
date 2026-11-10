import { describe, expect, test } from "bun:test";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, normalize, relative } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

/**
 * Who may import whom in the web host, read from every relative import.
 *
 * - The host (`src/`) imports no product, tool, fixture or test. Only the
 *   composition (`src/main.ts`, `src/app/`) mounts tools, and only the
 *   files in {@link COMPOSITION} name the products the host bundles.
 * - A product imports only its own folder, the code products share and
 *   the bridges.
 * - A bridge imports only other bridges: both sides read it, so it holds
 *   nothing of either.
 * - An integration (`src/integrations/<name>/`) is the host's side of one
 *   product's own protocol. It may use the host and the bridges, but no
 *   product, tool, fixture or other integration. The rest of the host
 *   imports neither it nor its bridge, `bridges/<name>/`; only the
 *   composition wires it in.
 *
 * {@link LEGACY} lists the imports that still break a rule. It only
 * shrinks: an entry that no longer exists fails the test too.
 */

const hostRoot = normalize(
  join(dirname(fileURLToPath(import.meta.url)), "../.."),
);

/** Code the products share, which is theirs and not the host's. */
const PRODUCT_SHARED = ["apps/shared"];

/** Each other folder of `apps/` is a product the host runs, its own owner. */
const PRODUCTS = readdirSync(join(hostRoot, "apps"), { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => `apps/${entry.name}`)
  .filter((folder) => !PRODUCT_SHARED.includes(folder));

/** The contracts and pure helpers both the host and the products read. */
const BRIDGES = ["bridges"];

/** Each folder of `src/integrations/` is the host side of one product's own protocol. */
const INTEGRATIONS = readdirSync(join(hostRoot, "src/integrations"), {
  withFileTypes: true,
})
  .filter((entry) => entry.isDirectory())
  .map((entry) => `src/integrations/${entry.name}`);

/** The bridge an integration owns: `bridges/<name>` for `src/integrations/<name>`. */
const integrationOfBridge = (path: string): string | undefined =>
  INTEGRATIONS.find((folder) =>
    under(path, `bridges/${folder.slice("src/integrations/".length)}`),
  );

/** The host files that name the products it bundles. */
const COMPOSITION = ["src/main.ts", "src/app/chat-products.ts"];

/** Imports that break a rule today, as `from -> to`. Remove an entry when it goes. */
const LEGACY: string[] = [];

type Owner =
  | { area: "host" | "bridge" | "shared" | "tools" | "fixtures" | "tests" }
  | { area: "product" | "integration"; name: string }
  | { area: "other" };

const under = (path: string, folder: string): boolean =>
  path === folder || path.startsWith(`${folder}/`);

function ownerOf(path: string): Owner {
  const product = PRODUCTS.find((folder) => under(path, folder));
  if (product !== undefined) return { area: "product", name: product };
  if (PRODUCT_SHARED.some((folder) => under(path, folder)))
    return { area: "shared" };
  if (BRIDGES.some((folder) => under(path, folder))) return { area: "bridge" };
  const integration = INTEGRATIONS.find((folder) => under(path, folder));
  if (integration !== undefined)
    return { area: "integration", name: integration };
  if (under(path, "src")) return { area: "host" };
  if (under(path, "tools")) return { area: "tools" };
  if (under(path, "fixtures")) return { area: "fixtures" };
  if (under(path, "tests") || under(path, "browser")) return { area: "tests" };
  return { area: "other" };
}

/** Why `from` may not import `to`, or null when it may. */
function broken(from: string, to: string): string | null {
  const source = ownerOf(from);
  const target = ownerOf(to);
  if (source.area === "integration") {
    if (target.area === "integration" && target.name !== source.name)
      return "an integration imports another integration";
    const owner = integrationOfBridge(to);
    if (owner !== undefined && owner !== source.name)
      return "an integration imports another integration's bridge";
    if (
      target.area === "host" ||
      target.area === "bridge" ||
      target.area === "integration"
    )
      return null;
    return "an integration imports a product, a tool, a fixture or a test";
  }
  if (source.area === "host") {
    const composition = COMPOSITION.includes(from);
    if (target.area === "integration")
      return composition ? null : "a host feature imports an integration";
    if (integrationOfBridge(to) !== undefined && !composition)
      return "a host feature imports an integration's bridge";
    if (target.area === "product" || target.area === "shared")
      return composition ? null : "the host imports a product";
    if (target.area === "tools")
      return composition || under(from, "src/app")
        ? null
        : "a host feature imports a tool";
    if (target.area === "fixtures" || target.area === "tests")
      return "the host imports a fixture or a test";
    return null;
  }
  if (source.area === "product" || source.area === "shared") {
    if (target.area === "bridge" || target.area === "shared") return null;
    if (target.area === "product")
      return source.area === "product" && target.name === source.name
        ? null
        : "a product imports another product";
    return "a product imports the host, a tool or a fixture";
  }
  if (source.area === "bridge")
    return target.area === "bridge"
      ? null
      : "a bridge imports something other than a bridge";
  return null;
}

/** Every TypeScript file under `folder`, relative to the host root. */
function sources(folder: string): string[] {
  const found: string[] = [];
  const walk = (dir: string): void => {
    let entries;
    try {
      entries = readdirSync(join(hostRoot, dir), { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      const path = `${dir}/${entry.name}`;
      if (entry.isDirectory()) {
        if (entry.name !== "node_modules" && entry.name !== "pkg") walk(path);
      } else if (entry.name.endsWith(".ts")) found.push(path);
    }
  };
  walk(folder);
  return found;
}

/** The relative imports of `file`, as paths from the host root, `.js` read as `.ts`. */
function importsOf(file: string): string[] {
  const text = readFileSync(join(hostRoot, file), "utf8");
  return ts
    .preProcessFile(text, true, true)
    .importedFiles.map((imported) => imported.fileName)
    .filter((name) => name.startsWith("."))
    .map((name) =>
      relative(hostRoot, join(hostRoot, dirname(file), name)).replace(
        /\.js$/,
        ".ts",
      ),
    );
}

/** Every import that breaks a rule, as `from -> to  (why)`. */
function violations(): Map<string, string> {
  const found = new Map<string, string>();
  const roots = new Set(
    [
      "src",
      "tools",
      "fixtures",
      ...PRODUCTS,
      ...PRODUCT_SHARED,
      ...BRIDGES,
    ].map((path) => path.split("/")[0]),
  );
  for (const root of roots)
    for (const file of sources(root))
      for (const target of importsOf(file)) {
        const why = broken(file, target);
        if (why !== null) found.set(`${file} -> ${target}`, why);
      }
  return found;
}

describe("web host boundaries", () => {
  const found = violations();

  test("no import breaks a rule beyond the listed legacy ones", () => {
    const fresh = [...found]
      .filter(([edge]) => !LEGACY.includes(edge))
      .map(([edge, why]) => `${edge}  (${why})`);
    expect(fresh).toEqual([]);
  });

  test("every legacy entry still exists, so the list only shrinks", () => {
    expect(LEGACY.filter((edge) => !found.has(edge))).toEqual([]);
  });

  test("the rules catch a product importing the host", () => {
    expect(broken(`${PRODUCTS[0]}/worker.ts`, "src/app/prompt.ts")).not.toBe(
      null,
    );
    expect(
      broken("src/products/product.ts", `${PRODUCTS[0]}/worker.ts`),
    ).not.toBe(null);
  });

  test("the rules keep the Nova integration apart from the rest of the host", () => {
    const nova = "src/integrations/nova-chat";
    expect(INTEGRATIONS).toContain(nova);
    expect({
      // General host code reaches neither the integration nor its protocol.
      walletChat: broken("src/invitations/wallet-chat.ts", `${nova}/start.ts`),
      loader: broken(
        "src/products/product.ts",
        "bridges/nova-chat/contract.ts",
      ),
      // The integration is host code: it reaches no product, tool or fixture.
      product: broken(`${nova}/host.ts`, "apps/nova-chat/session.ts"),
      tool: broken(`${nova}/host.ts`, "tools/chat/nova.ts"),
      fixture: broken(`${nova}/host.ts`, "fixtures/statements/x.ts"),
    }).toEqual({
      walletChat: "a host feature imports an integration",
      loader: "a host feature imports an integration's bridge",
      product: "an integration imports a product, a tool, a fixture or a test",
      tool: "an integration imports a product, a tool, a fixture or a test",
      fixture: "an integration imports a product, a tool, a fixture or a test",
    });
    expect({
      // It uses host services and its bridge; the composition wires it in.
      hostService: broken(`${nova}/start.ts`, "src/invitations/wallet-chat.ts"),
      ownBridge: broken(`${nova}/boundary.ts`, "bridges/nova-chat/wire.ts"),
      sharedBridge: broken(
        `${nova}/host.ts`,
        "bridges/primitives/statement.ts",
      ),
      composition: broken("src/main.ts", "bridges/nova-chat/contract.ts"),
    }).toEqual({
      hostService: null,
      ownBridge: null,
      sharedBridge: null,
      composition: null,
    });
  });
});
