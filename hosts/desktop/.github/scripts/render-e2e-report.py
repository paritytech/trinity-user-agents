#!/usr/bin/env python3
"""Render E2E results (allure-results + Playwright JUnit) as GitHub Markdown.

Modes:
  --mode=pr-comment    compact sticky PR comment: per-project donut rows with
                       Passed/Failed/Flaky/Skipped + collapsible details
  --mode=step-summary  detailed per-project/per-OS tables for
                       $GITHUB_STEP_SUMMARY (successor of render-e2e-summary.py)
  --mode=merge         merge per-shard allure-results into one directory for
                       the Allure CLI, injecting missing Platform parameters
  --mode=failure-media copy failed/flaky tests' screenshot/video attachments
                       into per-test dirs + manifest.json for the CI step that
                       uploads them as per-test artifacts (PR comment links)

Inputs are the *un-merged* artifact trees:
  --junit <dir>    e2e-junit-<project>-<os>/junit.xml
  --allure <dir>   allure-results-<project>-<os>/*-result.json

Attempts are grouped by (project, os, test name) — NOT by Allure historyId —
so a first attempt that dies before fixtures record the Platform parameter
still groups with its retry. See docs/_plans/2026-07-09-e2e-pr-report-design.md.
"""
import argparse
import json
import os
import shutil
import sys
import urllib.parse
import xml.etree.ElementTree as ET
from collections import defaultdict

OS_ORDER = ("linux", "macos", "windows")
OS_LABELS = {"linux": "Linux", "macos": "macOS", "windows": "Windows"}
# RUNNER_OS values as recorded by setupPlatformParameter() in e2e fixtures.
RUNNER_OS_VALUES = {"linux": "Linux", "macos": "macOS", "windows": "Windows"}

PASS, FAIL, SKIP = "✅", "❌", "⚠️"
FLAKY_MARK = "🔄"
DONUT_BASE = "https://allurecharts.qameta.workers.dev/pie"
MARKER = "<!-- e2e-report -->"
MAX_DETAIL_ENTRIES = 50

FAILED_STATUSES = ("failed", "broken")
GEN_SPEC_SUFFIX = ".feature.spec.js"
FEATURES_ROOT = "e2e/features/"
SECURITY_SUFFIX = ".e2e.ts"


def shard_meta(dirname, prefix):
    """'allure-results-product-sdk-linux' + 'allure-results-' -> ('product-sdk', 'linux').
    Project names contain hyphens, so split from the right on a known OS suffix;
    an unknown suffix falls back to linux (mirrors render-e2e-summary.py)."""
    if not dirname.startswith(prefix):
        return None
    rest = dirname[len(prefix):]
    head, _, tail = rest.rpartition("-")
    if tail in OS_ORDER and head:
        return head, tail
    return rest, "linux"


def is_infra(project):
    return project.startswith("setup-") or project.startswith("teardown")


def source_path(full_name):
    """Allure `fullName` -> repo source path.
    'e2e/features/smoke/x.feature.spec.js:6:3' -> 'e2e/features/smoke/x.feature'
    'security/probe.e2e.ts:46:3'               -> 'e2e/tests/security/probe.e2e.ts'"""
    if not full_name:
        return None
    path = full_name.rsplit(":", 2)[0]
    if path.endswith(GEN_SPEC_SUFFIX) and FEATURES_ROOT in path:
        return path[: -len(".spec.js")]
    if path.endswith(SECURITY_SUFFIX):
        return "e2e/tests/" + path
    return None


def attachment_media(result, shard_dir):
    """{'screenshot': path, 'video': path} for the failure-media attachments
    recorded by the e2e fixtures (attachFailureScreenshot / attachRecordedVideo)
    and Playwright's own on-failure screenshot. Screenshots match on exact
    name+type on purpose (BDD steps attach deliberate screenshots under custom
    names — not failure states); video matches on either signal.

    allure-playwright 3 records each testInfo.attach as an attachment STEP —
    the file reference lives in steps[].attachments, nested 1-2 levels deep
    (e.g. under the teardown fixture step); the result's top-level attachments
    carry only writer-added stdout. Both places are scanned."""
    media = {}

    def visit(attachments):
        for att in attachments or []:
            src = att.get("source")
            if not src:
                continue
            if att.get("name") == "screenshot" and att.get("type") == "image/png":
                media["screenshot"] = os.path.join(shard_dir, src)
            elif att.get("name") == "video" or att.get("type") == "video/webm":
                media["video"] = os.path.join(shard_dir, src)

    def walk(steps):
        for step in steps or []:
            visit(step.get("attachments"))
            walk(step.get("steps"))

    visit(result.get("attachments"))
    walk(result.get("steps"))
    return media


def collect_allure(root):
    """Walk allure-results-* shard dirs. Returns (tests, infra_fails):
    tests: (project, os, name) -> {"attempts": [(start, status, media)], "source": path|None}
    infra_fails: [(infra_project, test name, os)]"""
    tests = {}
    infra_fails = []
    if not root or not os.path.isdir(root):
        return tests, infra_fails
    for entry in sorted(os.listdir(root)):
        meta = shard_meta(entry, "allure-results-")
        if meta is None:
            continue
        project, surface = meta
        shard_dir = os.path.join(root, entry)
        if not os.path.isdir(shard_dir):
            continue
        for fname in sorted(os.listdir(shard_dir)):
            if not fname.endswith("-result.json"):
                continue
            try:
                with open(os.path.join(shard_dir, fname), encoding="utf-8") as f:
                    r = json.load(f)
            except (OSError, ValueError) as e:  # a broken file shouldn't sink the report
                print(f"<!-- render-e2e-report: failed to parse {entry}/{fname}: {e} -->", file=sys.stderr)
                continue
            name = r.get("name", "unknown")
            status = r.get("status", "unknown")
            start = r.get("start", 0)
            # Infra suites carry their project in the Project parameter (e.g. setup-chat).
            result_project = next(
                (p.get("value") for p in r.get("parameters", []) if p.get("name") == "Project"),
                project,
            )
            if is_infra(result_project):
                if status in FAILED_STATUSES:
                    infra_fails.append((result_project, name, surface))
                continue
            key = (project, surface, name)
            entry_data = tests.setdefault(key, {"attempts": [], "source": None})
            entry_data["attempts"].append((start, status, attachment_media(r, shard_dir)))
            entry_data["source"] = entry_data["source"] or source_path(r.get("fullName"))
    return tests, infra_fails


def finalize(tests):
    """Collapse attempts into one row per test."""
    rows = []
    for (project, surface, name), data in sorted(tests.items()):
        attempts = sorted(data["attempts"], key=lambda a: a[0])
        final = attempts[-1][1]
        earlier = [a[1] for a in attempts[:-1]]
        if final in FAILED_STATUSES:
            status, flaky = "failed", False
        elif final == "skipped":
            status, flaky = "skipped", False
        else:
            status = "passed"
            flaky = any(s in FAILED_STATUSES for s in earlier)
        rows.append({
            "project": project,
            "os": surface,
            "name": name,
            "status": status,
            "flaky": flaky,
            "source": data["source"],
        })
    return rows


# ---------------------------------------------------------------- junit input

def collect_junit(root):
    """Port of render-e2e-summary.py `collect`: walk e2e-junit-* shard dirs.
    Returns (product, features, infra_fails, tally, oses):
      product: project -> test name -> {os: icon}
      features: (project, test name) -> source path
      infra_fails: [(project, name, os)]
      tally: {PASS: n, FAIL: n, SKIP: n}
      oses: ordered list of OS present"""
    product = defaultdict(lambda: defaultdict(dict))
    features = {}
    infra_fails = []
    tally = {PASS: 0, FAIL: 0, SKIP: 0}
    oses_present = set()
    if not root or not os.path.isdir(root):
        return product, features, infra_fails, tally, []
    for entry in sorted(os.listdir(root)):
        meta = shard_meta(entry, "e2e-junit-")
        if meta is None:
            continue
        _, surface = meta
        junit = os.path.join(root, entry, "junit.xml")
        if not os.path.isfile(junit):
            continue
        try:
            xml_root = ET.parse(junit).getroot()
        except Exception as e:  # noqa: BLE001 - a broken shard shouldn't sink the summary
            print(f"<!-- render-e2e-report: failed to parse {junit}: {e} -->", file=sys.stderr)
            continue
        oses_present.add(surface)
        for suite in xml_root.iter("testsuite"):
            project = suite.get("hostname") or suite.get("name") or "(unknown)"
            for tc in suite.iter("testcase"):
                name = tc.get("name", "unknown")
                if tc.find("failure") is not None or tc.find("error") is not None:
                    icon = FAIL
                elif tc.find("skipped") is not None:
                    icon = SKIP
                else:
                    icon = PASS
                if is_infra(project):
                    if icon == FAIL:
                        infra_fails.append((project, name, surface))
                    continue
                tally[icon] += 1
                product[project][name][surface] = icon
                path = feature_path_from_classname(tc.get("classname"))
                if path:
                    features[(project, name)] = path
    ordered = [o for o in OS_ORDER if o in oses_present]
    return product, features, infra_fails, tally, ordered


def feature_path_from_classname(classname):
    """`../../.features-gen/<project>/e2e/features/.../x.feature.spec.js` ->
    `e2e/features/.../x.feature`. None for non-BDD suites."""
    if not classname or not classname.endswith(GEN_SPEC_SUFFIX):
        return None
    idx = classname.find(FEATURES_ROOT)
    if idx == -1:
        return None
    return classname[idx: -len(".spec.js")]


def rows_from_junit(root, collected=None):
    """Fallback rows (same shape as finalize()) when allure-results are absent.
    JUnit has final outcomes only — no attempt data, so flaky is always False
    and the caller must render the Flaky column as unknown (flaky_known=False).
    Pass a pre-collected collect_junit() tuple as `collected` to avoid a
    second parse of the same tree."""
    product, features, _, _, _ = collected if collected is not None else collect_junit(root)
    icon_status = {PASS: "passed", FAIL: "failed", SKIP: "skipped"}
    rows = []
    for project, by_name in product.items():
        for name, by_os in by_name.items():
            for surface, icon in by_os.items():
                rows.append({
                    "project": project,
                    "os": surface,
                    "name": name,
                    "status": icon_status[icon],
                    "flaky": False,
                    "source": features.get((project, name)),
                })
    # Same ordering as finalize(): (project, os, name).
    rows.sort(key=lambda r: (r["project"], r["os"], r["name"]))
    return rows


# ---------------------------------------------------------------- links

_feature_lines_cache = {}


def scenario_line(path, test_name):
    """Best-effort scenario line inside a .feature file (ported from
    render-e2e-summary.py). None for non-feature sources."""
    if not path or not path.endswith(".feature"):
        return None
    if path not in _feature_lines_cache:
        try:
            with open(path, encoding="utf-8") as f:
                _feature_lines_cache[path] = f.read().splitlines()
        except OSError:
            _feature_lines_cache[path] = None
    lines = _feature_lines_cache[path]
    if lines is None:
        return None
    title = test_name.rsplit(" › ", 1)[-1].strip()
    for n, line in enumerate(lines, start=1):
        stripped = line.strip()
        if stripped.startswith("Scenario") and stripped.endswith(title):
            return n
    return None


def linkify(test_name, path):
    """Wrap a test name into a link to its source at the ref this run was
    triggered from. Plain text outside GitHub Actions."""
    repo = os.environ.get("GITHUB_REPOSITORY")
    ref = (
        os.environ.get("GITHUB_HEAD_REF")
        or os.environ.get("GITHUB_REF_NAME")
        or os.environ.get("GITHUB_SHA")
    )
    if not path or not repo or not ref:
        return test_name
    server = os.environ.get("GITHUB_SERVER_URL", "https://github.com")
    url = f"{server}/{repo}/blob/{urllib.parse.quote(ref)}/{urllib.parse.quote(path)}"
    line = scenario_line(path, test_name)
    if line is not None:
        url += f"#L{line}"
    return f"[{test_name}]({url})"


def load_media(path):
    """manifest.json produced by --mode=failure-media and enriched with per-entry
    `url` by the CI upload step -> {(project, os, name): entry}. Entries without
    a url (upload failed / skipped) are unlinkable and dropped."""
    if not path or not os.path.isfile(path):
        return {}
    try:
        with open(path, encoding="utf-8") as f:
            manifest = json.load(f)
    except (OSError, ValueError) as e:
        print(f"<!-- render-e2e-report: failed to parse media manifest {path}: {e} -->", file=sys.stderr)
        return {}
    return {(e["project"], e["os"], e["name"]): e for e in manifest if e.get("url")}


# ---------------------------------------------------------------- pr-comment

def aggregate(rows):
    """Per-project counts across OS shards. flaky ⊂ passed."""
    agg = {}
    for row in rows:
        counts = agg.setdefault(row["project"], {"passed": 0, "failed": 0, "flaky": 0, "skipped": 0})
        counts[row["status"]] += 1
        if row["flaky"]:
            counts["flaky"] += 1
    return agg


def donut_img(passed, failed, flaky, skipped):
    """passed is the green (non-flaky-passed) segment; flaky ⊂ actual passed,
    so the alt text reconstructs the real passed count as passed + flaky."""
    url = (
        f"{DONUT_BASE}?passed={passed}&failed={failed}&broken={flaky}"
        f"&skipped={skipped}&unknown=0&size=32"
    )
    alt = f"{passed + flaky} passed, {failed} failed, {flaky} flaky, {skipped} skipped"
    return f'<img alt="{alt}" src="{url}" width="28px" height="28px" />'


def _detail_entry(row, media):
    location = f"({row['project']}, {OS_LABELS.get(row['os'], row['os'])})"
    entry = media.get((row["project"], row["os"], row["name"]))
    suffix = ""
    if entry:
        label = " + ".join(MEDIA_LABELS.get(f, f) for f in entry["files"])
        suffix = f" · [{label}]({entry['url']})"
    return f"- {linkify(row['name'], row['source'])} {location}{suffix}"


def _detail_section(title, rows, media):
    out = ["<details>", f"<summary>{title}</summary>", ""]
    for row in rows[:MAX_DETAIL_ENTRIES]:
        out.append(_detail_entry(row, media))
    if len(rows) > MAX_DETAIL_ENTRIES:
        out.append(f"- …and {len(rows) - MAX_DETAIL_ENTRIES} more")
    out.extend(["", "</details>", ""])
    return out


def _infra_fails_section(infra_fails):
    out = ["### Setup / teardown failures", "", "| Project | Test | OS |", "|---|---|---|"]
    for project, name, surface in sorted(infra_fails):
        out.append(f"| {project} | {name} | {OS_LABELS.get(surface, surface)} {FAIL} |")
    out.append("")
    return out


def render_pr_comment(agg, rows, infra_fails, links, flaky_known, media=None):
    """links: {"launch": url|"", "run": url|""}.
    flaky_known=False (junit fallback) renders the Flaky column as —.
    media: {(project, os, name): {url, files, ...}} (optional)."""
    media = media or {}
    out = [MARKER, "## E2E Tests", ""]

    if not agg:
        out.append("_No E2E results found — every shard likely failed before producing results. Check the job logs._")
        if links.get("run"):
            out.append("")
            out.append(f"[Run summary]({links['run']})")
        if infra_fails:
            out.append("")
            out.extend(_infra_fails_section(infra_fails))
        return "\n".join(out)

    totals = {"passed": 0, "failed": 0, "flaky": 0, "skipped": 0}
    for counts in agg.values():
        for k in totals:
            totals[k] += counts[k]

    flaky_total = str(totals["flaky"]) if flaky_known else "—"
    green = totals["passed"] - totals["flaky"] if flaky_known else totals["passed"]
    out.append(
        f"{donut_img(green, totals['failed'], totals['flaky'] if flaky_known else 0, totals['skipped'])} "
        f"**{totals['passed']} passed · {totals['failed']} failed · "
        f"{flaky_total} flaky · {totals['skipped']} skipped**"
    )

    link_parts = []
    if links.get("launch"):
        link_parts.append(f"[TestOps launch]({links['launch']})")
    if links.get("run"):
        link_parts.append(f"[Run summary]({links['run']})")
    if link_parts:
        out.append("")
        out.append(" · ".join(link_parts))

    out.extend(["", "|  | Project | Passed | Failed | Flaky | Skipped |", "|---|---|---|---|---|---|"])
    for project in sorted(agg):
        c = agg[project]
        flaky_cell = str(c["flaky"]) if flaky_known else "—"
        c_green = c["passed"] - c["flaky"] if flaky_known else c["passed"]
        img = donut_img(c_green, c["failed"], c["flaky"] if flaky_known else 0, c["skipped"])
        out.append(f"| {img} | {project} | {c['passed']} | {c['failed']} | {flaky_cell} | {c['skipped']} |")
    out.append("")

    failed_rows = [r for r in rows if r["status"] == "failed"]
    flaky_rows = [r for r in rows if r["flaky"]]
    if failed_rows:
        out.extend(_detail_section(f"❌ Failed ({len(failed_rows)})", failed_rows, media))
    if flaky_rows:
        out.extend(_detail_section(f"🔄 Flaky ({len(flaky_rows)})", flaky_rows, media))

    if infra_fails:
        out.extend(_infra_fails_section(infra_fails))

    return "\n".join(out)


# ---------------------------------------------------------------- step summary

def flaky_name_set(rows):
    """(project, os, allure test name) for every flaky row. JUnit BDD names are
    'Feature › Scenario' while allure names are the bare scenario title, so the
    junit side compares its last ` › ` segment against this set."""
    return {(r["project"], r["os"], r["name"]) for r in rows if r["flaky"]}


def roll_up(rows, oses):
    """One status per OS for a project: ❌ if any test failed, ⚠️ if all
    present tests skipped, ✅ otherwise. Missing OS -> no cell."""
    per_os = {}
    for surface in oses:
        outcomes = [rows[t][surface] for t in rows if surface in rows[t]]
        if not outcomes:
            continue
        if FAIL in outcomes:
            per_os[surface] = FAIL
        elif all(o == SKIP for o in outcomes):
            per_os[surface] = SKIP
        else:
            per_os[surface] = PASS
    return per_os


def render_step_summary(product, features, infra_fails, tally, oses, flaky_names):
    out = ["## E2E Test Results", ""]

    total = tally[PASS] + tally[FAIL] + tally[SKIP]
    if total == 0:
        out.append(
            "_No JUnit results found — every e2e shard likely failed before "
            "producing results. Check the job logs._"
        )
        return "\n".join(out)

    out.append(
        f"📊 **{tally[PASS]} passed · {tally[FAIL]} failed · "
        f"{tally[SKIP]} skipped** ({total} total)"
    )
    out.append("")

    headers = " | ".join(OS_LABELS[o] for o in oses)
    sep = "|".join(["---"] * (len(oses) + 1))

    out.append("### Coverage")
    out.append("")
    out.append(f"| Project | {headers} |")
    out.append(f"|{sep}|")
    for project in sorted(product):
        per_os = roll_up(product[project], oses)
        cells = " | ".join(per_os.get(o, "—") for o in oses)
        out.append(f"| {project} | {cells} |")
    out.append("")

    for project in sorted(product):
        out.append(f"### {project}")
        out.append("")
        out.append(f"| Test | {headers} |")
        out.append(f"|{sep}|")
        for name in sorted(product[project]):
            row = product[project][name]
            title = name.rsplit(" › ", 1)[-1].strip()
            cells = []
            for o in oses:
                icon = row.get(o, "—")
                if icon == PASS and (project, o, title) in flaky_names:
                    icon = f"{PASS} {FLAKY_MARK}"
                cells.append(icon)
            label = linkify(name, features.get((project, name)))
            out.append(f"| {label} | {' | '.join(cells)} |")
        out.append("")

    if infra_fails:
        out.append("### Setup / teardown failures")
        out.append("")
        out.append("| Project | Test | OS |")
        out.append("|---|---|---|")
        for project, name, surface in sorted(infra_fails):
            out.append(f"| {project} | {name} | {OS_LABELS[surface]} {FAIL} |")
        out.append("")

    return "\n".join(out)


# ---------------------------------------------------------------- merge

def merge_shards(allure_root, out_dir):
    """Flat-copy every shard's files into out_dir for the Allure CLI. Result
    files missing the Platform parameter get it injected from the shard's OS —
    a first attempt that died before fixtures ran lacks it, which would split
    its retry into a separate (falsely failed) test in the generated report.
    Returns the number of normalized result files."""
    os.makedirs(out_dir, exist_ok=True)
    fixed = 0
    if not allure_root or not os.path.isdir(allure_root):
        return fixed
    for entry in sorted(os.listdir(allure_root)):
        meta = shard_meta(entry, "allure-results-")
        shard_dir = os.path.join(allure_root, entry)
        if meta is None or not os.path.isdir(shard_dir):
            continue
        _, surface = meta
        platform = RUNNER_OS_VALUES.get(surface, "Linux")
        for fname in sorted(os.listdir(shard_dir)):
            src = os.path.join(shard_dir, fname)
            dst = os.path.join(out_dir, fname)
            if not fname.endswith("-result.json"):
                shutil.copyfile(src, dst)
                continue
            try:
                with open(src, encoding="utf-8") as f:
                    result = json.load(f)
            except (OSError, ValueError):
                shutil.copyfile(src, dst)
                continue
            parameters = result.setdefault("parameters", [])
            if not any(p.get("name") == "Platform" for p in parameters):
                parameters.append({"name": "Platform", "value": platform})
                fixed += 1
            with open(dst, "w", encoding="utf-8") as f:
                json.dump(result, f)
    return fixed


# ---------------------------------------------------------------- failure media

MEDIA_FILES = (("screenshot", "screenshot.png"), ("video", "video.webm"))
MEDIA_LABELS = {"screenshot.png": "📸 screenshot", "video.webm": "📹 video"}


def write_failure_media(tests, out_dir, cap=MAX_DETAIL_ENTRIES):
    """Copy failed/flaky tests' screenshot/video attachments into out_dir/<n>/
    and write out_dir/manifest.json describing them. Screenshot and video are
    each taken from the LATEST attempt that has one (failed test -> failing
    run's media; flaky test -> failed attempt's screenshot + passing retry's
    video). Failed rows take priority over flaky when the cap cuts the list.
    Returns the manifest."""
    rows = finalize(tests)
    candidates = [r for r in rows if r["status"] == "failed"] + [r for r in rows if r["flaky"]]
    manifest = []
    dropped = 0
    for row in candidates:
        if len(manifest) >= cap:
            dropped += 1
            continue
        attempts = sorted(tests[(row["project"], row["os"], row["name"])]["attempts"], key=lambda a: a[0])
        entry_dir = str(len(manifest))
        files = []
        for kind, fname in MEDIA_FILES:
            src = next((a[2][kind] for a in reversed(attempts) if a[2].get(kind)), None)
            if not src:
                continue
            if not os.path.isfile(src):
                print(f"<!-- render-e2e-report: media file missing: {src} -->", file=sys.stderr)
                continue
            os.makedirs(os.path.join(out_dir, entry_dir), exist_ok=True)
            shutil.copyfile(src, os.path.join(out_dir, entry_dir, fname))
            files.append(fname)
        if files:
            manifest.append({"project": row["project"], "os": row["os"], "name": row["name"],
                             "dir": entry_dir, "files": files})
    if dropped:
        print(f"<!-- render-e2e-report: media cap reached, dropped {dropped} test(s) -->", file=sys.stderr)
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f)
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", required=True, choices=["pr-comment", "step-summary", "merge", "failure-media"])
    parser.add_argument("--junit", help="junit artifacts root (e2e-junit-* subdirs)")
    parser.add_argument("--allure", help="allure-results artifacts root (allure-results-* subdirs)")
    parser.add_argument("--out", help="output dir for --mode=merge / --mode=failure-media")
    parser.add_argument("--launch-url", default="")
    parser.add_argument("--run-url", default="")
    parser.add_argument("--media", default="", help="failure-media manifest.json (from --mode=failure-media, url-enriched by CI)")
    args = parser.parse_args()

    links = {"launch": args.launch_url, "run": args.run_url}

    if args.mode == "failure-media":
        if not args.out:
            parser.error("--mode=failure-media requires --out")
        manifest = write_failure_media(collect_allure(args.allure)[0], args.out)
        print(f"collected failure media for {len(manifest)} test(s) into {args.out}")
        return

    if args.mode == "pr-comment":
        tests, infra_fails = collect_allure(args.allure)
        rows = finalize(tests)
        flaky_known = True
        if not rows:
            junit_data = collect_junit(args.junit)
            rows = rows_from_junit(args.junit, collected=junit_data)
            infra_fails = infra_fails or junit_data[2]
            flaky_known = False
        print(render_pr_comment(aggregate(rows), rows, infra_fails, links, flaky_known,
                                media=load_media(args.media)))
        return

    if args.mode == "step-summary":
        product, features, infra_fails, tally, oses = collect_junit(args.junit)
        flaky_names = flaky_name_set(finalize(collect_allure(args.allure)[0]))
        print(render_step_summary(product, features, infra_fails, tally, oses or ["linux"], flaky_names))
        return

    if args.mode == "merge":
        if not args.out:
            parser.error("--mode=merge requires --out")
        fixed = merge_shards(args.allure, args.out)
        print(f"merged allure shards into {args.out}; normalized {fixed} result file(s)")
        return


if __name__ == "__main__":
    main()
