import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import {
  brotliCompressSync,
  constants as zlibConstants,
  gzipSync,
} from "node:zlib";

/**
 * Parse an asset list: one group per line, as `<name> <dir> [!<prefix>...]`.
 * The directory is relative to the repository root, and each `!` prefix
 * excludes the paths under the directory that start with it. Blank lines and
 * `#` comments are skipped.
 */
export function parseAssets(text) {
  return text
    .split("\n")
    .map((line) => line.replace(/#.*/, "").trim())
    .filter(Boolean)
    .map((line) => {
      const [name, dir, ...rest] = line.split(/\s+/);
      if (!dir || rest.some((p) => !p.startsWith("!"))) {
        throw new Error(
          `expected "<name> <dir> [!<prefix>...]", got "${line}"`,
        );
      }
      return { name, dir, exclude: rest.map((p) => p.slice(1)) };
    });
}

const ASSET = /\.(js|cjs|mjs|wasm)$/;

/**
 * A file's name with the per-build hash wasm-bindgen puts in its snippet
 * directories removed, so the same snippet compares across commits.
 */
export function stableName(name) {
  return name.replace(
    /(^|\/)snippets\/([^/]+)-[0-9a-f]{16}\//,
    "$1snippets/$2/",
  );
}

/** Raw, gzip and brotli byte counts, at the levels `build-wasm.mjs` ships. */
export function measureFile(path) {
  const bytes = readFileSync(path);
  return {
    raw: bytes.length,
    gzip: gzipSync(bytes, { level: 9 }).length,
    brotli: brotliCompressSync(bytes, {
      params: { [zlibConstants.BROTLI_PARAM_QUALITY]: 11 },
    }).length,
  };
}

/**
 * Measure the JS and WASM files of every group under `root`, keyed by
 * `<group>/<path>`. Throws naming each group that has none, so a skipped build
 * step cannot pass as a size drop.
 */
export function measureGroups(root, groups) {
  const files = {};
  const missing = [];
  for (const { name, dir, exclude = [] } of groups) {
    const path = join(root, dir);
    const found = existsSync(path)
      ? readdirSync(path, { recursive: true })
          .filter((f) => ASSET.test(f) && !exclude.some((p) => f.startsWith(p)))
          .sort()
      : [];
    if (found.length === 0) missing.push(`${name} (${dir})`);
    for (const file of found) {
      files[`${name}/${stableName(file)}`] = {
        group: name,
        ...measureFile(join(path, file)),
      };
    }
  }
  if (missing.length > 0) {
    throw new Error(`no build output for: ${missing.join(", ")}`);
  }
  return files;
}

/** Human-readable size, in the units `build-wasm.mjs` prints. */
export function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  const kib = bytes / 1024;
  if (kib < 1024) return `${kib.toFixed(1)} KiB`;
  return `${(kib / 1024).toFixed(2)} MiB`;
}

function formatDelta(current, base) {
  const diff = current - (base ?? current);
  if (diff === 0) return "";
  const sign = diff > 0 ? "+" : "-";
  const pct = base === 0 ? 0 : Math.abs((diff / base) * 100);
  const share = pct >= 0.05 ? `, ${sign}${pct.toFixed(1)}%` : "";
  return ` (${sign}${formatBytes(Math.abs(diff))}${share})`;
}

function totals(files) {
  const zero = () => ({ raw: 0, gzip: 0, brotli: 0 });
  const byGroup = {};
  const all = zero();
  for (const entry of Object.values(files)) {
    for (const sum of [(byGroup[entry.group] ??= zero()), all]) {
      sum.raw += entry.raw;
      sum.gzip += entry.gzip;
      sum.brotli += entry.brotli;
    }
  }
  return { byGroup, all };
}

const HEADER = ["| | Raw | Gzip | Brotli |", "|---|---:|---:|---:|"];

/**
 * Markdown comparing `current` to `baseline` (both `{ commit, files }`
 * snapshots); `baseline` may be absent. `repoUrl` turns commits into links.
 */
export function renderReport({ current, baseline, repoUrl }) {
  const base = baseline?.files;
  const link = (sha) =>
    repoUrl
      ? `[${sha.slice(0, 7)}](${repoUrl}/commit/${sha})`
      : `\`${sha.slice(0, 7)}\``;
  const row = (label, cur, old) => {
    const note = base && cur && !old ? " (new)" : "";
    const cell = (field) =>
      cur
        ? `${formatBytes(cur[field])}${formatDelta(cur[field], old?.[field])}`
        : "removed";
    return `| ${label}${note} | ${cell("raw")} | ${cell("gzip")} | ${cell("brotli")} |`;
  };
  const table = (names, cur, old) => [
    ...HEADER,
    ...names.map((name) => row(`\`${name}\``, cur[name], old?.[name])),
  ];

  const cur = totals(current.files);
  const old = base && totals(base);
  const groups = Object.keys({ ...cur.byGroup, ...old?.byGroup });
  const lines = ["## Bundle size report", ""];
  if (!base) {
    lines.push(
      "> No baseline from `main` was found, so sizes are shown without a comparison.",
      "",
    );
  } else if (baseline.commit) {
    lines.push(`Compared with \`main\` at ${link(baseline.commit)}.`, "");
  }
  lines.push(
    ...table(groups, cur.byGroup, old?.byGroup),
    row("**Total**", cur.all, old?.all),
    "",
  );

  const wasm = Object.keys(current.files).filter((n) => n.endsWith(".wasm"));
  lines.push(
    "**WebAssembly modules**",
    "",
    ...table(wasm, current.files, base),
    "",
  );

  // With a baseline, the files that changed, largest change first; without
  // one, every file.
  const listed = base
    ? Object.keys({ ...current.files, ...base })
        .map((n) => [n, (current.files[n]?.raw ?? 0) - (base[n]?.raw ?? 0)])
        .filter(([, diff]) => diff !== 0)
        .sort((a, b) => Math.abs(b[1]) - Math.abs(a[1]))
        .map(([n]) => n)
    : Object.keys(current.files);
  if (listed.length === 0) {
    lines.push("No file changed size.", "");
  } else {
    const title = base ? "Changed files" : "All files";
    lines.push(
      "<details>",
      `<summary>${title} (${listed.length})</summary>`,
      "",
      ...table(listed, current.files, base),
      "",
      "</details>",
      "",
    );
  }

  if (current.commit) lines.push(`<sub>Commit: ${link(current.commit)}</sub>`);
  return `${lines.join("\n")}\n`;
}
