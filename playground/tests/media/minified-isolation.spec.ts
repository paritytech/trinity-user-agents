import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { build as esbuild } from "esbuild";
import { rolldown } from "rolldown";

// Hosts ship the backend through aggressive minifiers. dotli's production
// build (Vite 8: rolldown + oxc) assumes property reads are free of side
// effects, so an isolation probe that relies on a read throwing is deleted.
// Each variant must still tell a cross-origin product frame from a
// same-origin one in a real browser.
const core = new URL("../../../", import.meta.url);
const backend = fileURLToPath(
  new URL("js/packages/truapi-host/dist/web/browser-media-backend.js", core),
);

async function esbuildMinified(): Promise<string> {
  const result = await esbuild({
    entryPoints: [backend],
    bundle: true,
    minify: true,
    format: "esm",
    external: ["neverthrow"],
    write: false,
  });
  return result.outputFiles[0]!.text;
}

// Mirrors dotli's `rolldownOptions()` (config/vite/src/build-options.ts).
async function rolldownMinified(): Promise<string> {
  const treeshake = {
    propertyReadSideEffects: false as const,
    unknownGlobalSideEffects: false,
  };
  const bundle = await rolldown({
    input: backend,
    external: ["neverthrow"],
    treeshake,
  });
  try {
    const { output } = await bundle.generate({
      format: "esm",
      minify: {
        compress: {
          treeshake: { ...treeshake, invalidImportSideEffects: false },
        },
        mangle: { toplevel: true },
        codegen: { removeWhitespace: true },
      },
    });
    return output[0].code;
  } finally {
    await bundle.close();
  }
}

const fixture = `
import { createBrowserMediaBackend } from "/backend.mjs";
const product = { productId: "media-isolation.dot", executionKind: "App" };
const allow = "camera 'none'; microphone 'none'; display-capture 'none'; fullscreen 'none'";
const cases = {
  opaqueSandbox: { sandbox: "allow-scripts", srcdoc: "Untrusted product" },
  crossOrigin: { sandbox: "allow-scripts allow-same-origin", src: "http://127.0.0.1:48197/product" },
  sameOrigin: { sandbox: "allow-scripts allow-same-origin", srcdoc: "Readable product" },
};
window.isolation = async () => {
  const verdicts = {};
  let runtime = 0n;
  for (const [name, setup] of Object.entries(cases)) {
    const mount = document.createElement("section");
    mount.style.cssText = "position:relative;isolation:isolate;width:320px;height:200px";
    const frame = document.createElement("iframe");
    frame.sandbox = setup.sandbox;
    frame.allow = allow;
    frame.style.cssText = "position:absolute;inset:0;z-index:1;width:100%;height:100%;border:0";
    const { promise: loaded, resolve } = Promise.withResolvers();
    frame.addEventListener("load", resolve, { once: true });
    if (setup.src) frame.src = setup.src;
    else frame.srcdoc = setup.srcdoc;
    mount.append(frame);
    document.body.append(mount);
    await loaded;
    const media = createBrowserMediaBackend({
      window, document, productId: product.productId,
      getProductElement: () => frame,
      getCompositorMount: () => mount,
      // The host asserts isolation for every frame; only the backend's own
      // origin check may refuse the readable one.
      isProductIsolated: () => true,
      indicatorMount: document.querySelector("#controls"),
      requestConsent: async () => { throw new Error("Unexpected consent request"); },
    });
    try {
      media.attach(++runtime);
      verdicts[name] = "attached";
    } catch (error) {
      verdicts[name] = error.failure?.value?.error?.tag ?? error.failure?.tag ?? String(error);
    } finally {
      media.dispose();
      mount.remove();
    }
  }
  return verdicts;
};
`;

const html = `<!doctype html><div id="controls"></div>
<script type="importmap">{"imports":{"neverthrow":"/neverthrow.mjs"}}</script>
<script type="module" src="/fixture.mjs"></script>`;

const variants = {
  tsc: () => readFile(backend, "utf8"),
  esbuild: esbuildMinified,
  rolldown: rolldownMinified,
};

for (const [variant, compile] of Object.entries(variants)) {
  test(`${variant} build refuses only a readable product frame`, async ({
    page,
  }) => {
    const resources: Record<string, string> = {
      "/backend.mjs": await compile(),
      "/neverthrow.mjs": await readFile(
        new URL("node_modules/neverthrow/dist/index.es.js", core),
        "utf8",
      ),
      "/fixture.mjs": fixture,
    };
    await page.route("http://localhost:48196/**", async (route) => {
      const source = resources[new URL(route.request().url()).pathname];
      await route.fulfill(
        source === undefined
          ? { contentType: "text/html", body: html }
          : { contentType: "text/javascript", body: source },
      );
    });
    await page.route("http://127.0.0.1:48197/**", (route) =>
      route.fulfill({ contentType: "text/html", body: "Remote product" }),
    );
    await page.goto("http://localhost:48196/");
    await page.waitForFunction("!!window.isolation");
    expect(await page.evaluate("window.isolation()")).toEqual({
      opaqueSandbox: "attached",
      crossOrigin: "attached",
      sameOrigin: "SurfaceUnavailable",
    });
  });
}
