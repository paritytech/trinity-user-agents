// Lets an installed bulletin-deploy publish `includes.funding` as the object
// the core reads, where its validator takes only a boolean.
//
// Usage: node scripts/patch-bulletin-deploy.mjs <path to node_modules/bulletin-deploy>
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = process.argv[2];
if (!root) {
  console.error(
    "usage: patch-bulletin-deploy.mjs <bulletin-deploy package dir>",
  );
  process.exit(2);
}

const edits = [
  [
    'if (inc.funding !== void 0 && typeof inc.funding !== "boolean") {',
    'if (inc.funding !== void 0 && typeof inc.funding !== "boolean" && !isPlainObject(inc.funding)) {',
  ],
  [
    "inc.funding !== true) {",
    "inc.funding !== true && !isPlainObject(inc.funding)) {",
  ],
];

const dist = join(root, "dist");
let patched = 0;
for (const name of readdirSync(dist).filter((file) => file.endsWith(".js"))) {
  const path = join(dist, name);
  const source = readFileSync(path, "utf8");
  if (!source.includes("includes.funding must be a boolean when present"))
    continue;
  let next = source;
  for (const [from, to] of edits) {
    if (next.includes(to)) continue;
    if (!next.includes(from)) {
      console.error(`${name}: validator changed shape, patch by hand`);
      process.exit(1);
    }
    next = next.replace(from, to);
  }
  if (next !== source) writeFileSync(path, next);
  patched += 1;
}
if (patched === 0) {
  console.error(
    "no includes.funding validator found; this bulletin-deploy may accept the object already",
  );
  process.exit(1);
}
console.log(`patched ${patched} file(s)`);
