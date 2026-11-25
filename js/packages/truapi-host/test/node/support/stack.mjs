import { readFileSync } from "node:fs";

const stacks = new URL("../../../../../../stacks/", import.meta.url);

/**
 * The stack `name` as a host build bundles it: the link and signed copy in
 * the repository's `stacks/`.
 */
export function bundledStack(name) {
  return {
    pointer: readFileSync(new URL(`${name}.link`, stacks), "utf8").trim(),
    descriptor: new Uint8Array(readFileSync(new URL(`${name}.json`, stacks))),
  };
}
