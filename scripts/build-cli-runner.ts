import { mkdir, mkdtemp, rename, rm } from "node:fs/promises";
import { join, resolve } from "node:path";

const repository = resolve(import.meta.dir, "..");
const destination = resolve(process.argv[2] ?? join(repository, "target/dist"));
await mkdir(destination, { recursive: true });
const staging = await mkdtemp(join(destination, ".runner-"));
try {
  for (const [entrypoint, filename, target, format] of [
    ["runner.ts", "runner.js", "bun", "esm"],
    ["browser-sandbox.ts", "sandbox-assets/container.js", "browser", "iife"],
  ] as const) {
    const result = await Bun.build({
      entrypoints: [
        join(repository, "rust/crates/truapi-host-cli/js", entrypoint),
      ],
      target,
      format,
      env: "disable",
      define: { "process.env.NODE_ENV": '"production"' },
    });
    if (!result.success || result.outputs.length !== 1) {
      throw new Error(`Cannot bundle ${entrypoint}: ${result.logs.join("\n")}`);
    }
    await Bun.write(join(staging, filename), result.outputs[0]);
  }
  for (const name of ["runner.js", "sandbox-assets"]) {
    await rm(join(destination, name), { recursive: true, force: true });
    await rename(join(staging, name), join(destination, name));
  }
} finally {
  await rm(staging, { recursive: true, force: true });
}
