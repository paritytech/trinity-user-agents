import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

// Execute the workflow's actual shell steps against small Git histories and
// stubbed external commands. No GitHub writes or registry requests leave tests.
function step(workflow, name) {
  const yaml = readFileSync(
    new URL(`../../.github/workflows/${workflow}.yml`, import.meta.url),
    "utf8",
  );
  const start = yaml.indexOf(`      - name: ${name}\n`);
  assert.notEqual(start, -1, `step ${name} exists`);
  const run = yaml.indexOf("        run: |\n", start);
  assert.notEqual(run, -1, `step ${name} has a script`);
  return yaml
    .slice(run + "        run: |\n".length)
    .match(/^(?: {10}[^\n]*\n|\n)+/)[0]
    .replace(/^ {10}/gm, "");
}

const filter = step("ci", "Detect which areas changed");
const release = step("ci", "A version bump must consume every changeset");
const changeset = step("ci", "Released code needs a changeset");
const compare = step(
  "registry-drift",
  "Compare every published manifest against npm",
);
const report = step("registry-drift", "Report the drift");
const iosFallback = step(
  "registry-drift",
  "Compare the iOS published fallback against its newest release",
);
const checkCrate = step("release", "Check the crate version against crates.io");
const confirmCrate = step("release", "Confirm the crate reached crates.io");
const title =
  "Release drift: published versions do not match the default branch";
const manifest = "js/packages/truapi/package.json";

function fixture(t, { pending = false } = {}) {
  const root = mkdtempSync(join(tmpdir(), "release-consistency-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const cwd = join(root, "repo");
  const bin = join(root, "bin");
  mkdirSync(cwd);
  mkdirSync(bin);
  const write = (path, contents) => {
    const target = join(cwd, path);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, contents);
  };
  const git = (...args) =>
    execFileSync("git", args, {
      cwd,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }).trim();
  const commit = () => {
    git("add", "-A");
    git(
      "-c",
      "user.name=Test",
      "-c",
      "user.email=test@example.com",
      "-c",
      "commit.gpgsign=false",
      "commit",
      "-qm",
      "fixture",
      "--allow-empty",
    );
    return git("rev-parse", "HEAD");
  };
  const version = (value) =>
    write(manifest, JSON.stringify({ name: "@parity/truapi", version: value }));
  git("init", "-q");
  version("1.0.0");
  write(".changeset/README.md", "Changesets documentation\n");
  write(".github/registry-drift-exceptions.json", "{}\n");
  if (pending)
    write(
      ".changeset/pending.md",
      '---\n"@parity/truapi": patch\n---\nFix a bug.\n',
    );
  const base = commit();
  const stub = (command, source) =>
    writeFileSync(join(bin, command), `#!/usr/bin/env node\n${source}`, {
      mode: 0o755,
    });
  // Every external command fails closed unless a test supplies its behavior.
  stub("gh", 'throw new Error("unexpected GitHub call");');
  stub("npm", 'throw new Error("unexpected registry call");');
  const execute = (script, env = {}) => {
    const output = join(root, "output");
    writeFileSync(output, "");
    const result = spawnSync("bash", ["-c", script], {
      cwd,
      encoding: "utf8",
      env: {
        ...process.env,
        PATH: `${bin}:${process.env.PATH}`,
        GITHUB_OUTPUT: output,
        RUNNER_TEMP: root,
        EVENT_NAME: "pull_request",
        MERGE_BASE: base,
        PUSH_BASE: base,
        GH_REPO: "test/repository",
        PR_NUMBER: "747",
        ADDS_CHANGESET: "false",
        TITLE: title,
        REPORT: "@parity/truapi@1.0.0",
        WORKFLOW_URL: "https://example.com/registry-drift",
        ...env,
      },
    });
    return { ...result, output: readFileSync(output, "utf8") };
  };
  return { root, cwd, write, git, commit, version, base, stub, execute };
}

function passed(result) {
  assert.equal(result.status, 0, result.stdout + result.stderr);
}

for (const path of [
  "rust/crates/truapi-codegen/src/emitter.rs",
  "rust/crates/truapi-host-cli/src/main.rs",
  "scripts/codegen.sh",
  "nightly-toolchain",
  "scripts/bundle-truapi-dts.mjs",
  "scripts/regen-explorer-versions.mjs",
  "Cargo.toml",
  "Cargo.lock",
  "package.json",
  "package-lock.json",
  "js/packages/truapi-host/package.json",
  "js/packages/truapi-host/scripts/build-wasm.mjs",
  "js/packages/truapi/tsconfig.json",
  "js/packages/truapi/src/client.ts",
]) {
  test(`changeset required for ${path}`, (t) => {
    const f = fixture(t);
    f.write(path, "changed\n");
    f.commit();
    const result = f.execute(filter);
    passed(result);
    assert.match(result.output, /^needs_changeset=true$/m);
    assert.match(result.output, /^adds_changeset=false$/m);
  });
}

test("documentation changes do not require a changeset", (t) => {
  const f = fixture(t);
  f.write("docs/RELEASE_PROCESS.md", "new docs\n");
  f.commit();
  const result = f.execute(filter);
  passed(result);
  assert.match(result.output, /^needs_changeset=false$/m);
});

for (const operation of ["add", "delete", "edit", "rename", "readme"]) {
  test(`changeset evidence: ${operation}`, (t) => {
    const f = fixture(t, { pending: true });
    const pending = join(f.cwd, ".changeset/pending.md");
    if (operation === "add")
      f.write(
        ".changeset/new.md",
        '---\n"@parity/truapi": minor\n---\nA new feature.\n',
      );
    if (operation === "delete") rmSync(pending);
    if (operation === "edit") f.write(".changeset/pending.md", "edited\n");
    if (operation === "rename")
      renameSync(pending, join(f.cwd, ".changeset/renamed.md"));
    if (operation === "readme") {
      rmSync(join(f.cwd, ".changeset/README.md"));
      f.commit();
      f.write(".changeset/README.md", "documentation\n");
    }
    f.commit();
    const result = f.execute(filter);
    passed(result);
    assert.match(
      result.output,
      new RegExp(`^adds_changeset=${operation === "add"}$`, "m"),
    );
  });
}

test("a rerun reads changed PR metadata and does not use a stale opt-out", (t) => {
  const f = fixture(t);
  const metadata = join(f.root, "pr.json");
  f.stub(
    "gh",
    `process.stdout.write(require("node:fs").readFileSync(${JSON.stringify(metadata)}));`,
  );
  for (const [pr, status] of [
    [{ title: "fix: behavior", labels: [] }, 1],
    [{ title: "fix: behavior", labels: [{ name: "ci:no-changeset" }] }, 0],
    [{ title: "release: @parity/truapi 1.0.0", labels: [] }, 0],
    [{ title: "fix: behavior", labels: [] }, 1],
  ]) {
    writeFileSync(metadata, JSON.stringify(pr));
    const result = f.execute(changeset);
    assert.equal(result.status, status, result.stdout + result.stderr);
  }
  passed(f.execute(changeset, { ADDS_CHANGESET: "true" }));
});

test("PR metadata lookup errors fail the guard", (t) => {
  const f = fixture(t);
  f.stub("gh", "process.exit(1);");
  assert.notEqual(f.execute(changeset).status, 0);
});

for (const event of [
  "pull_request",
  "merge_group",
  "push",
  "workflow_dispatch",
]) {
  test(`${event} rejects a bump with a pending changeset`, (t) => {
    const f = fixture(t, { pending: true });
    f.version("1.1.0");
    f.commit();
    const result = f.execute(release, { EVENT_NAME: event });
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stdout, /Rebuild the release commit/);
  });
}

test("a release that passes on its branch fails after merging a late changeset", (t) => {
  const f = fixture(t, { pending: true });
  f.git("checkout", "-qb", "release");
  f.version("1.1.0");
  rmSync(join(f.cwd, ".changeset/pending.md"));
  f.commit();
  passed(f.execute(release));

  f.git("checkout", "-qb", "updated-base", f.base);
  f.write(
    ".changeset/late.md",
    '---\n"@parity/truapi": patch\n---\nLate fix.\n',
  );
  const updatedBase = f.commit();
  f.git(
    "-c",
    "user.name=Test",
    "-c",
    "user.email=test@example.com",
    "-c",
    "commit.gpgsign=false",
    "merge",
    "--no-ff",
    "release",
    "-m",
    "queued merge",
  );
  for (const event of ["pull_request", "merge_group"]) {
    const result = f.execute(release, {
      EVENT_NAME: event,
      MERGE_BASE: updatedBase,
    });
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stdout, /\.changeset\/late\.md/);
  }
});

test("a consumed release passes with Changesets' README still present", (t) => {
  const f = fixture(t, { pending: true });
  f.version("1.1.0");
  rmSync(join(f.cwd, ".changeset/pending.md"));
  f.commit();
  passed(f.execute(release));
});

test("a source change without a bump may leave pending changesets", (t) => {
  const f = fixture(t, { pending: true });
  f.write("rust/crates/truapi/src/lib.rs", "new code\n");
  f.commit();
  passed(f.execute(release));
});

test("unknown merge bases and missing shallow parents fail closed", (t) => {
  const f = fixture(t);
  assert.notEqual(
    f.execute(release, { EVENT_NAME: "merge_group", MERGE_BASE: "missing" })
      .status,
    0,
  );
  assert.notEqual(f.execute(release).status, 0);
});

test("malformed base manifests fail closed", (t) => {
  const f = fixture(t);
  f.write(manifest, "{broken");
  f.commit();
  f.version("1.1.0");
  f.commit();
  assert.notEqual(f.execute(release).status, 0);
});

test("a moved manifest still triggers the release guard", (t) => {
  const f = fixture(t, { pending: true });
  f.version("1.1.0");
  renameSync(
    join(f.cwd, "js/packages/truapi"),
    join(f.cwd, "js/packages/renamed"),
  );
  f.commit();
  const result = f.execute(release);
  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stdout, /@parity\/truapi 1.0.0 -> 1.1.0/);
});

test("new and private packages do not look like an existing release bump", (t) => {
  const f = fixture(t, { pending: true });
  f.write(
    "js/packages/new/package.json",
    '{"name":"@parity/new","version":"0.1.0"}',
  );
  f.write("js/packages/private/package.json", '{"private":true}');
  f.commit();
  passed(f.execute(release));
});

test("a multi-commit push is compared against the pre-push revision", (t) => {
  const f = fixture(t, { pending: true });
  f.version("1.1.0");
  f.commit();
  f.write("README.md", "second commit\n");
  f.commit();
  assert.equal(f.execute(release, { EVENT_NAME: "push" }).status, 1);
});

for (const response of ["published", "E404", "E500", "ETIMEDOUT", "invalid"]) {
  test(`registry result: ${response}`, (t) => {
    const f = fixture(t);
    f.stub(
      "npm",
      `
      console.error("npm diagnostic on stderr");
      console.log(${JSON.stringify(response === "published" ? '"1.0.0"' : response === "invalid" ? "unparseable response" : JSON.stringify({ error: { code: response } }))});
      process.exit(${response === "published" ? 0 : 1});
    `,
    );
    const result = f.execute(compare);
    if (response === "published" || response === "E404") {
      passed(result);
      assert.match(
        result.output,
        new RegExp(`^drift=${response === "E404"}$`, "m"),
      );
    } else {
      assert.equal(result.status, 1);
      assert.equal(result.output, "");
    }
  });
}

test("a registry error after a missing version never emits a partial drift report", (t) => {
  const f = fixture(t);
  f.write(
    "js/packages/a/package.json",
    '{"name":"@parity/a","version":"1.0.0"}',
  );
  f.stub(
    "npm",
    'console.log(JSON.stringify({error:{code: process.argv[3].startsWith("@parity/a@") ? "E404" : "E500"}})); process.exit(1);',
  );
  const result = f.execute(compare);
  assert.equal(result.status, 1);
  assert.equal(result.output, "");
});

test("intentional omissions apply only to the documented package version", (t) => {
  const f = fixture(t);
  f.write(
    ".github/registry-drift-exceptions.json",
    '{"@parity/truapi@1.0.0":"Deferred release"}',
  );
  passed(f.execute(compare));
  f.version("1.1.0");
  f.stub(
    "npm",
    'console.log(JSON.stringify({error:{code:"E404"}})); process.exit(1);',
  );
  const result = f.execute(compare);
  passed(result);
  assert.match(result.output, /drift=true/);
  assert.match(result.output, /@parity\/truapi@1.1.0/);
});

test("exceptions require an explanation", (t) => {
  const f = fixture(t);
  f.write(
    ".github/registry-drift-exceptions.json",
    '{"@parity/truapi@1.0.0":" "}',
  );
  assert.equal(f.execute(compare).status, 1);
});

test("private packages are not queried against npm", (t) => {
  const f = fixture(t);
  f.write(manifest, '{"private":true}');
  const result = f.execute(compare);
  passed(result);
  assert.match(result.output, /drift=false/);
});

test("drift reporting finds later pages and avoids duplicate issues and comments", (t) => {
  const f = fixture(t);
  const calls = join(f.root, "calls.jsonl");
  const pages = join(f.root, "issues.json");
  f.stub(
    "gh",
    `
    const fs = require("node:fs");
    const args = process.argv.slice(2);
    fs.appendFileSync(${JSON.stringify(calls)}, JSON.stringify(args) + "\\n");
    if (args[0] === "api") process.stdout.write(fs.readFileSync(${JSON.stringify(pages)}));
    else if (args[0] !== "issue" || !["edit", "create"].includes(args[1])) throw new Error("unexpected write");
  `,
  );
  const listed = [
    { number: 2, title, pull_request: {} },
    { number: 3, title: `${title} elsewhere` },
  ];
  writeFileSync(
    pages,
    `${JSON.stringify(listed)}\n${JSON.stringify([{ number: 747, title, body: "outdated report" }])}`,
  );
  assert.equal(f.execute(report).status, 1);
  let requests = readFileSync(calls, "utf8").trim().split("\n").map(JSON.parse);
  assert.deepEqual(requests[0], [
    "api",
    "--paginate",
    "repos/test/repository/issues?state=open&per_page=100",
  ]);
  assert.deepEqual(requests[1].slice(0, 3), ["issue", "edit", "747"]);
  assert.equal(requests[1][3], "--body-file");
  const body = readFileSync(requests[1][4], "utf8");
  assert.match(body, /```\n@parity\/truapi@1.0.0\n```/);

  writeFileSync(calls, "");
  writeFileSync(pages, JSON.stringify([{ number: 747, title, body }]));
  assert.equal(f.execute(report).status, 1);
  requests = readFileSync(calls, "utf8").trim().split("\n").map(JSON.parse);
  assert.equal(requests.length, 1, "unchanged report makes no writes");

  writeFileSync(calls, "");
  writeFileSync(pages, JSON.stringify(listed));
  assert.equal(f.execute(report).status, 1);
  requests = readFileSync(calls, "utf8").trim().split("\n").map(JSON.parse);
  assert.deepEqual(requests[1].slice(0, 4), [
    "issue",
    "create",
    "--title",
    title,
  ]);
});

test("issue listing errors do not create a duplicate", (t) => {
  const f = fixture(t);
  const calls = join(f.root, "calls");
  f.stub(
    "gh",
    `require("node:fs").appendFileSync(${JSON.stringify(calls)}, process.argv[2] + "\\n"); process.exit(1);`,
  );
  assert.equal(f.execute(report).status, 1);
  assert.equal(readFileSync(calls, "utf8"), "api\n");
});


const packageSwift = (version) =>
  `let publishedBinaryURL = "https://github.com/paritytech/trinity-user-agents/releases/download/%40parity%2Fios-host%40${version}/truapi_server.xcframework.zip"\n`;

// gh is invoked with --jq, so it emits one tag per line rather than JSON.
const releases = (...tags) =>
  `console.log(${JSON.stringify(tags.join("\n"))});`;

test("the iOS fallback naming the newest release is not drift", (t) => {
  const f = fixture(t);
  f.write("Package.swift", packageSwift("0.16.0"));
  f.stub("gh", releases("@parity/ios-host@0.16.0", "@parity/truapi@0.16.0"));
  passed(f.execute(iosFallback));
  assert.equal(existsSync(join(f.root, "ios-drift.txt")), false);
});

test("an older iOS fallback is recorded for the drift report", (t) => {
  const f = fixture(t);
  f.write("Package.swift", packageSwift("0.7.0"));
  f.stub("gh", releases("@parity/ios-host@0.7.0", "@parity/ios-host@0.16.0"));
  passed(f.execute(iosFallback));
  assert.match(
    readFileSync(join(f.root, "ios-drift.txt"), "utf8"),
    /@parity\/ios-host@0\.16\.0 \(Package\.swift falls back to 0\.7\.0\)/,
  );
});

test("a pre-release is never treated as the newest iOS release", (t) => {
  const f = fixture(t);
  f.write("Package.swift", packageSwift("0.16.0"));
  f.stub("gh", releases("@parity/ios-host@0.16.0", "@parity/ios-host@0.17.0-beta.1"));
  passed(f.execute(iosFallback));
  assert.equal(existsSync(join(f.root, "ios-drift.txt")), false);
});

test("no published iOS release at all is not drift, and does not kill the step", (t) => {
  const f = fixture(t);
  f.write("Package.swift", packageSwift("0.16.0"));
  f.stub("gh", releases("@parity/truapi@1.0.0"));
  passed(f.execute(iosFallback));
  assert.equal(existsSync(join(f.root, "ios-drift.txt")), false);
});

test("a manifest naming no iOS release fails rather than reporting no drift", (t) => {
  const f = fixture(t);
  f.write("Package.swift", "let publishedBinaryURL = \"https://example.com/nothing.zip\"\n");
  f.stub("gh", releases("@parity/ios-host@0.16.0"));
  assert.equal(f.execute(iosFallback).status, 1);
});

test("recorded iOS drift reaches the report through the npm comparison", (t) => {
  const f = fixture(t);
  writeFileSync(
    join(f.root, "ios-drift.txt"),
    "@parity/ios-host@0.16.0 (Package.swift falls back to 0.7.0)\n",
  );
  f.stub("npm", 'console.log(JSON.stringify("1.0.0"));');
  const result = f.execute(compare);
  assert.equal(result.status, 0);
  assert.match(result.output, /drift=true/);
  assert.match(result.output, /falls back to 0\.7\.0/);
});

// crates.io sparse index answers, served in order and the last one repeated.
const indexListing = (...versions) => ({ status: 200, versions });
const indexMissing = { status: 404, versions: [] };
const indexFailure = { status: 503, versions: [] };
const crateIndex = (...answers) => `
const fs = require("node:fs");
const answers = ${JSON.stringify(answers)};
const args = process.argv.slice(2);
fs.appendFileSync(__filename + ".urls", args.at(-1) + "\\n");
const call = fs.readFileSync(__filename + ".urls", "utf8").trim().split("\\n").length - 1;
const answer = answers[Math.min(call, answers.length - 1)];
fs.writeFileSync(
  args[args.indexOf("-o") + 1],
  answer.versions.map((vers) => JSON.stringify({ name: "trinity-stack", vers })).join("\\n"),
);
process.stdout.write(String(answer.status));
`;

function crateFixture(context, { manifest = "0.3.0", answers }) {
  const repository = fixture(context);
  repository.write(
    "rust/crates/trinity-stack/Cargo.toml",
    `[package]\nname = "trinity-stack"\nversion = "${manifest}"\n`,
  );
  const indexScript = join(repository.cwd, "scripts/crates-io-index.sh");
  repository.write(
    "scripts/crates-io-index.sh",
    readFileSync(new URL("../crates-io-index.sh", import.meta.url)),
  );
  chmodSync(indexScript, 0o755);
  repository.stub("curl", crateIndex(...answers));
  const run = (script, env = {}) =>
    repository.execute(script, {
      VERSION: "0.3.0",
      CRATES_INDEX_INTERVAL_SECONDS: "1",
      ...env,
    });
  const urlLog = join(repository.root, "bin/curl.urls");
  const lookups = () =>
    existsSync(urlLog) ? readFileSync(urlLog, "utf8").trim().split("\n") : [];
  return { run, lookups };
}

test("a crate manifest that differs from the released version fails the crate publish", (context) => {
  const crate = crateFixture(context, {
    manifest: "0.2.0",
    answers: [indexMissing],
  });
  const result = crate.run(checkCrate);
  assert.equal(result.status, 1);
  assert.match(
    result.stdout,
    /Cargo\.toml is 0\.2\.0, but the release declares 0\.3\.0/,
  );
  assert.equal(result.output, "");
});

test("a trinity-stack version already on crates.io is not published again", (context) => {
  const crate = crateFixture(context, {
    answers: [indexListing("0.2.0", "0.3.0")],
  });
  const result = crate.run(checkCrate);
  passed(result);
  assert.match(result.output, /^listed=true$/m);
  assert.deepEqual(crate.lookups(), [
    "https://index.crates.io/tr/in/trinity-stack",
  ]);
});

for (const [label, answer] of [
  ["the first trinity-stack version", indexMissing],
  ["a new trinity-stack version", indexListing("0.2.0", "0.3.0-rc.1")],
]) {
  test(`${label} is sent to crates.io`, (context) => {
    const crate = crateFixture(context, { answers: [answer] });
    const result = crate.run(checkCrate);
    passed(result);
    assert.match(result.output, /^listed=false$/m);
  });
}

test("a crates.io index error fails the crate publish", (context) => {
  const crate = crateFixture(context, { answers: [indexFailure] });
  const result = crate.run(checkCrate);
  assert.equal(result.status, 1);
  assert.equal(result.output, "");
});

test("the crate publish is confirmed once the index lists the version", (context) => {
  const crate = crateFixture(context, {
    answers: [indexMissing, indexFailure, indexListing("0.3.0")],
  });
  const result = crate.run(confirmCrate, { CRATES_INDEX_WAIT_SECONDS: "30" });
  passed(result);
  assert.equal(crate.lookups().length, 3);
});

test("the crate publish fails when the version is not on crates.io by the deadline", (context) => {
  const crate = crateFixture(context, { answers: [indexMissing] });
  const result = crate.run(confirmCrate, { CRATES_INDEX_WAIT_SECONDS: "1" });
  assert.equal(result.status, 1);
  assert.match(result.stdout, /trinity-stack 0\.3\.0 is not on crates\.io/);
});
