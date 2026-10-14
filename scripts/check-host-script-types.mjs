#!/usr/bin/env node
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const runnerDirectory = fileURLToPath(
  new URL("../rust/crates/truapi-host-cli/js/", import.meta.url),
);
const config = ts.readConfigFile(
  join(runnerDirectory, "tsconfig.json"),
  ts.sys.readFile,
);
if (config.error)
  throw new Error(
    ts.flattenDiagnosticMessageText(config.error.messageText, "\n"),
  );
const parsed = ts.parseJsonConfigFileContent(
  config.config,
  ts.sys,
  runnerDirectory,
);
const [template, declarations] = await Promise.all([
  readFile(join(runnerDirectory, "scratch.ts"), "utf8"),
  readFile(join(runnerDirectory, "script-types.d.ts"), "utf8"),
]);
const directory = await mkdtemp(join(tmpdir(), "truapi-script-types-"));

try {
  const scripts = [];
  for (const name of ["first", "second"]) {
    const typesName = `${name}.types.d.ts`;
    const script = join(directory, `${name}.ts`);
    await writeFile(join(directory, typesName), declarations);
    await writeFile(script, template.replace("__TRUAPI_TYPES__", typesName));
    scripts.push(script);
  }
  const program = ts.createProgram(
    [...parsed.fileNames, ...scripts],
    parsed.options,
  );
  const diagnostics = [...parsed.errors, ...ts.getPreEmitDiagnostics(program)];
  if (diagnostics.length) {
    console.error(
      ts.formatDiagnosticsWithColorAndContext(diagnostics, {
        getCanonicalFileName: (filename) => filename,
        getCurrentDirectory: () => process.cwd(),
        getNewLine: () => "\n",
      }),
    );
    process.exitCode = 1;
  } else {
    console.log(
      "Host script type fixture and independent scratch scripts passed.",
    );
  }
} finally {
  await rm(directory, { recursive: true, force: true });
}
