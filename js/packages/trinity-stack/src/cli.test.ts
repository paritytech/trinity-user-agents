import { describe, expect, test } from "bun:test";
import { mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { knownStacks } from "./index.js";

const CLI = new URL("./cli.ts", import.meta.url).pathname;

function runCli(...args: string[]) {
  const { exitCode, stdout, stderr } = Bun.spawnSync([
    process.execPath,
    CLI,
    ...args,
  ]);
  return { exitCode, stdout: stdout.toString(), stderr: stderr.toString() };
}

describe("trinity-stack CLI", () => {
  test("known prints each known stack's name and link", () => {
    const expected = Object.entries(knownStacks)
      .map(([name, link]) => {
        const mark = name === "westend" ? " (unsigned)" : "";
        return `${name}${mark}\n  ${link}\n`;
      })
      .join("");
    expect(runCli("known")).toEqual({
      exitCode: 0,
      stdout: expected,
      stderr: "",
    });
  });

  // A mistyped name must not cause a fetch or leave a half-written project behind.
  test("add with a name alone fails for an unknown stack and writes nothing", async () => {
    const dir = await mkdtemp(join(tmpdir(), "trinity-stack-cli-"));
    try {
      const { exitCode, stderr } = runCli("add", "not-known", "--dir", dir);
      expect(exitCode).toBe(1);
      expect(stderr).toContain(
        `known stacks: ${Object.keys(knownStacks).join(", ")}`,
      );
      expect(await readdir(dir)).toEqual([]);
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });
});
