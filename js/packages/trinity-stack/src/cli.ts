#!/usr/bin/env node
// `trinity-stack add | update | list | remove`: the build copies of a project's stacks.
import { parseArgs } from "node:util";
import { add, list, remove, update } from "./project.js";

const USAGE = `usage:
  trinity-stack add <name> <link | url> [--key 0x..]
  trinity-stack update [<name>...]
  trinity-stack list
  trinity-stack remove <name>

options:
  --dir <path>  where the stacks live, .trinity by default

update exits 0 when nothing changed, 2 when it wrote something and 1 when a stack failed,
which keeps its copies while the others update. It reads dotNS over the stack's eth-rpc, so a
stack with a dotNS gateway and no eth-rpc fails.`;

const { positionals, values } = parseArgs({
  allowPositionals: true,
  options: {
    dir: { type: "string" },
    key: { type: "string" },
    help: { type: "boolean", short: "h" },
  },
});
const [command, ...rest] = positionals;
const options = { dir: values.dir };

try {
  if (values.help || !command) {
    console.log(USAGE);
  } else if (command === "add" && rest.length === 2) {
    const { changed } = await add(rest[0], rest[1], values.key, options);
    console.log(`wrote ${changed.join(", ")}`);
  } else if (command === "update") {
    const { changed, failures } = await update(rest, options);
    console.log(
      changed.length > 0 ? `wrote ${changed.join(", ")}` : "nothing changed",
    );
    for (const failure of failures) console.error(failure);
    process.exitCode = failures.length > 0 ? 1 : changed.length > 0 ? 2 : 0;
  } else if (command === "list" && rest.length === 0) {
    for (const { name, link, descriptor, dotns } of await list(options)) {
      const copy = descriptor
        ? `sequence ${descriptor.sequence}, generation ${descriptor.generation}`
        : "no copy";
      const contracts = dotns ? `, dotNS read at #${dotns.at.number}` : "";
      console.log(`${name}  ${copy}${contracts}\n  ${link}`);
    }
  } else if (command === "remove" && rest.length === 1) {
    const { changed } = await remove(rest[0], options);
    console.log(`wrote ${changed.join(", ")}`);
  } else {
    console.error(USAGE);
    process.exitCode = 1;
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
}
