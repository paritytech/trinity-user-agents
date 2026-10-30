import { resolve } from "node:path";

const vendor = resolve(import.meta.dirname, "vendor");

/** The aliases tsconfig.json declares as `paths`, for the bundler: getcash's
 *  packages and app helpers from the pinned checkout, and the descriptors
 *  generated from its metadata. */
export const aliases = [
  {
    find: /^@getsome\/([a-z-]+)$/,
    replacement: `${vendor}/getcash-community/packages/$1/src/index.ts`,
  },
  {
    find: /^@getcash\/lib\/(.+)$/,
    replacement: `${vendor}/getcash-community/lib/$1`,
  },
  {
    find: "@polkadot-api/descriptors",
    replacement: `${vendor}/papi/descriptors/dist/index.js`,
  },
];

/** Every variable whose name starts with this is read at build time. */
export const envPrefix = "FUNDTEST_";
