#!/usr/bin/env node
// `trinity-stack add | update | generate | list | known | remove`: the build copies of a project's stacks.
import { parseArgs } from "node:util";
import { decodeLink, knownStacks } from "./index.js";
import { add, generate, list, remove, update } from "./project.js";

const USAGE = `usage:
  trinity-stack add <name> [<link | url>] [--key 0x..]
  trinity-stack update [<name>...]
  trinity-stack generate
  trinity-stack list
  trinity-stack known
  trinity-stack remove <name>

options:
  --dir <path>  where the stacks live, .trinity by default

add without a link uses the link of the known stack <name>; known prints each known stack and
its link. An unsigned known stack, marked (unsigned), is listed for its link only: a build copy
must be signed, so add refuses it.

update exits 0 when no committed file changed, 2 when one did and 1 when a stack failed; a
stack that fails is left as it was while the others update.

generate rebuilds index.js and index.d.ts from the committed copies without fetching, for a
"prepare" script; .trinity/.gitignore leaves both out of git.`;

try {
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
  if (values.help || !command) {
    console.log(USAGE);
  } else if (command === "add" && (rest.length === 1 || rest.length === 2)) {
    const { changed, warnings } = await add(
      rest[0],
      rest[1],
      values.key,
      options,
    );
    console.log(`wrote ${changed.join(", ")}`);
    for (const warning of warnings) console.error(warning);
  } else if (command === "update") {
    const { changed, failures, warnings } = await update(rest, options);
    console.log(
      changed.length > 0 ? `wrote ${changed.join(", ")}` : "nothing changed",
    );
    for (const line of [...warnings, ...failures]) console.error(line);
    process.exitCode = failures.length > 0 ? 1 : changed.length > 0 ? 2 : 0;
  } else if (command === "generate" && rest.length === 0) {
    const { changed } = await generate(options);
    console.log(
      changed.length > 0 ? `wrote ${changed.join(", ")}` : "nothing changed",
    );
  } else if (command === "list" && rest.length === 0) {
    for (const { name, link, descriptor } of await list(options)) {
      const copy = descriptor
        ? `sequence ${descriptor.sequence}, generation ${descriptor.generation}`
        : "no copy";
      console.log(`${name}  ${copy}\n  ${link}`);
    }
  } else if (command === "known" && rest.length === 0) {
    for (const [name, link] of Object.entries(knownStacks))
      console.log(
        `${name}${decodeLink(link).key ? "" : " (unsigned)"}\n  ${link}`,
      );
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
