#!/usr/bin/env python3
"""Unit tests for render-e2e-report.py. Run: python3 -m unittest discover .github/scripts -p 'test_*.py' -v"""
import contextlib
import importlib.util
import io
import json
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(__file__)
spec = importlib.util.spec_from_file_location("render_e2e_report", os.path.join(HERE, "render-e2e-report.py"))
rer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rer)


def write_result(shard_dir, name, status, start, *, project_param=None, full_name=None, uuid_hint="",
                 attachments=None, steps=None):
    os.makedirs(shard_dir, exist_ok=True)
    result = {
        "uuid": f"{uuid_hint}{start}-{status}",
        "name": name,
        "status": status,
        "start": start,
        "stop": start + 100,
        "labels": [],
        "parameters": [],
    }
    if project_param:
        result["parameters"].append({"name": "Project", "value": project_param})
    if full_name:
        result["fullName"] = full_name
    if attachments:
        result["attachments"] = attachments
    if steps:
        result["steps"] = steps
    path = os.path.join(shard_dir, f"{uuid_hint}{start}-{status}-result.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump(result, f)


def write_attachment(shard_dir, fname, data=b"x"):
    os.makedirs(shard_dir, exist_ok=True)
    with open(os.path.join(shard_dir, fname), "wb") as f:
        f.write(data)


SCREENSHOT_ATT = {"name": "screenshot", "source": "s1-attachment.png", "type": "image/png"}
VIDEO_ATT = {"name": "video", "source": "v1-attachment.webm", "type": "video/webm"}


class ShardMetaTest(unittest.TestCase):
    def test_project_with_hyphens(self):
        self.assertEqual(rer.shard_meta("allure-results-product-sdk-linux", "allure-results-"), ("product-sdk", "linux"))

    def test_junit_prefix(self):
        self.assertEqual(rer.shard_meta("e2e-junit-security-macos", "e2e-junit-"), ("security", "macos"))

    def test_rejects_foreign_dir(self):
        self.assertIsNone(rer.shard_meta("something-else", "allure-results-"))

    def test_unknown_os_suffix_falls_back_to_linux(self):
        self.assertEqual(rer.shard_meta("allure-results-smoke", "allure-results-"), ("smoke", "linux"))


class GroupingTest(unittest.TestCase):
    def make_root(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        return self.tmp.name

    def test_failed_then_passed_is_flaky_not_failed(self):
        # The PR #730 false-failed regression: attempts differ in parameters,
        # grouping by name must still unite them.
        root = self.make_root()
        shard = os.path.join(root, "allure-results-security-linux")
        write_result(shard, "blocks fetch to fake IPFS domain", "failed", 1000)
        write_result(shard, "blocks fetch to fake IPFS domain", "passed", 2000)
        tests, infra = rer.collect_allure(root)
        rows = rer.finalize(tests)
        self.assertEqual(len(rows), 1)
        row = rows[0]
        self.assertEqual(row["status"], "passed")
        self.assertTrue(row["flaky"])
        self.assertEqual(row["project"], "security")
        self.assertEqual(infra, [])

    def test_failed_after_retry_stays_failed(self):
        root = self.make_root()
        shard = os.path.join(root, "allure-results-smoke-linux")
        write_result(shard, "TC-1 boots", "failed", 1000)
        write_result(shard, "TC-1 boots", "broken", 2000)
        rows = rer.finalize(rer.collect_allure(root)[0])
        self.assertEqual(rows[0]["status"], "failed")
        self.assertFalse(rows[0]["flaky"])

    def test_single_skip(self):
        root = self.make_root()
        write_result(os.path.join(root, "allure-results-chat-linux"), "TC-7 decline", "skipped", 1000)
        rows = rer.finalize(rer.collect_allure(root)[0])
        self.assertEqual(rows[0]["status"], "skipped")

    def test_same_name_across_os_not_grouped(self):
        root = self.make_root()
        write_result(os.path.join(root, "allure-results-security-linux"), "probe", "passed", 1000)
        write_result(os.path.join(root, "allure-results-security-macos"), "probe", "failed", 1000)
        rows = rer.finalize(rer.collect_allure(root)[0])
        self.assertEqual(len(rows), 2)
        statuses = {(r["os"], r["status"]) for r in rows}
        self.assertEqual(statuses, {("linux", "passed"), ("macos", "failed")})

    def test_infra_suites_excluded_and_failures_surfaced(self):
        root = self.make_root()
        write_result(os.path.join(root, "allure-results-chat-linux"), "provision pool", "passed", 1000,
                     project_param="setup-chat")
        write_result(os.path.join(root, "allure-results-chat-linux"), "delete users", "failed", 2000,
                     project_param="teardown-bot-users")
        tests, infra = rer.collect_allure(root)
        self.assertEqual(rer.finalize(tests), [])
        self.assertEqual(infra, [("teardown-bot-users", "delete users", "linux")])


class SourcePathTest(unittest.TestCase):
    def test_bdd_spec_maps_to_feature_file(self):
        self.assertEqual(
            rer.source_path("e2e/features/smoke/address-bar.feature.spec.js:6:3"),
            "e2e/features/smoke/address-bar.feature",
        )

    def test_security_spec_maps_to_e2e_tests(self):
        self.assertEqual(
            rer.source_path("security/deep-link-injection.e2e.ts:46:3"),
            "e2e/tests/security/deep-link-injection.e2e.ts",
        )

    def test_unknown_returns_none(self):
        self.assertIsNone(rer.source_path("weird/thing.spec.ts:1:1"))
        self.assertIsNone(rer.source_path(None))


class PrCommentTest(unittest.TestCase):
    def make_root(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        return self.tmp.name

    def media_manifest(self, tmp_path, entries):
        path = os.path.join(tmp_path, "manifest.json")
        with open(path, "w", encoding="utf-8") as f:
            json.dump(entries, f)
        return path

    def build_rows(self):
        root = self.make_root()
        smoke = os.path.join(root, "allure-results-smoke-linux")
        write_result(smoke, "TC-1 boots", "failed", 1000)
        write_result(smoke, "TC-1 boots", "passed", 2000)          # flaky
        write_result(smoke, "TC-2 renders", "passed", 1000)
        write_result(smoke, "TC-3 skipped one", "skipped", 1000)
        sec_l = os.path.join(root, "allure-results-security-linux")
        sec_m = os.path.join(root, "allure-results-security-macos")
        write_result(sec_l, "probe", "passed", 1000)
        write_result(sec_m, "probe", "failed", 1000,
                     full_name="security/probe.e2e.ts:1:1")        # finally failed
        return rer.finalize(rer.collect_allure(root)[0])

    def test_aggregate_sums_across_os(self):
        agg = rer.aggregate(self.build_rows())
        self.assertEqual(agg["smoke"], {"passed": 2, "failed": 0, "flaky": 1, "skipped": 1})
        self.assertEqual(agg["security"], {"passed": 1, "failed": 1, "flaky": 0, "skipped": 0})

    def test_comment_structure(self):
        rows = self.build_rows()
        body = rer.render_pr_comment(
            rer.aggregate(rows), rows, [],
            {"launch": "https://testops.example.com/launch/1", "run": "https://gh/run/2"},
            flaky_known=True,
        )
        self.assertTrue(body.startswith(rer.MARKER))
        self.assertIn("## E2E Tests", body)
        # totals: 3 passed, 1 failed, 1 flaky, 1 skipped
        self.assertIn("3 passed · 1 failed · 1 flaky · 1 skipped", body)
        # donut: green segment excludes flaky (totals: 3 passed of which 1 flaky)
        self.assertIn("pie?passed=2&failed=1&broken=1&skipped=1&unknown=0&size=32", body)
        # per-project rows present, no OS columns, no New/Retry
        self.assertIn("| smoke |", body)
        self.assertIn("| security |", body)
        self.assertNotIn("Retry", body)
        self.assertNotIn("New", body)
        # links line
        self.assertIn("[TestOps launch](https://testops.example.com/launch/1)", body)
        self.assertIn("[Run summary](https://gh/run/2)", body)
        self.assertNotIn("Allure report", body)
        # details sections
        self.assertIn("❌ Failed (1)", body)
        self.assertIn("🔄 Flaky (1)", body)
        self.assertIn("(security, macOS)", body)

    def test_links_omitted_when_missing(self):
        rows = self.build_rows()
        body = rer.render_pr_comment(rer.aggregate(rows), rows, [], {"launch": "", "run": ""}, flaky_known=True)
        self.assertNotIn("TestOps launch", body)
        self.assertNotIn("Run summary", body)

    def test_infra_failures_section(self):
        rows = self.build_rows()
        body = rer.render_pr_comment(rer.aggregate(rows), rows, [("setup-chat", "provision", "linux")],
                                     {"launch": "", "run": ""}, flaky_known=True)
        self.assertIn("Setup / teardown failures", body)
        self.assertIn("setup-chat", body)

    def test_empty_state_keeps_marker(self):
        body = rer.render_pr_comment({}, [], [], {"launch": "", "run": ""}, flaky_known=True)
        self.assertTrue(body.startswith(rer.MARKER))
        self.assertIn("No E2E results found", body)

    def test_empty_state_still_shows_infra_failures(self):
        # A run where only setup/teardown failed must not hide those failures
        # behind the "No E2E results found" empty state.
        body = rer.render_pr_comment(
            {}, [], [("setup-chat", "provision", "linux")],
            {"launch": "", "run": ""}, flaky_known=True,
        )
        self.assertTrue(body.startswith(rer.MARKER))
        self.assertIn("No E2E results found", body)
        self.assertIn("Setup / teardown failures", body)
        self.assertIn("setup-chat", body)

    def test_media_links_on_failed_and_flaky_rows(self):
        rows = self.build_rows()
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        path = self.media_manifest(tmp.name, [
            {"project": "security", "os": "macos", "name": "probe",
             "dir": "0", "files": ["screenshot.png", "video.webm"], "url": "https://gh/a/1"},
            {"project": "smoke", "os": "linux", "name": "TC-1 boots",
             "dir": "1", "files": ["video.webm"], "url": "https://gh/a/2"},
        ])
        media = rer.load_media(path)
        body = rer.render_pr_comment(rer.aggregate(rows), rows, [],
                                     {"launch": "", "run": ""}, flaky_known=True, media=media)
        self.assertIn("· [📸 screenshot + 📹 video](https://gh/a/1)", body)
        self.assertIn("· [📹 video](https://gh/a/2)", body)

    def test_entry_without_url_renders_no_link(self):
        rows = self.build_rows()
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        path = self.media_manifest(tmp.name, [
            {"project": "security", "os": "macos", "name": "probe",
             "dir": "0", "files": ["screenshot.png"]},
        ])
        self.assertEqual(rer.load_media(path), {})
        body = rer.render_pr_comment(rer.aggregate(rows), rows, [],
                                     {"launch": "", "run": ""}, flaky_known=True, media=rer.load_media(path))
        self.assertNotIn("📸", body)

    def test_load_media_missing_or_broken_returns_empty(self):
        self.assertEqual(rer.load_media(None), {})
        self.assertEqual(rer.load_media("/nonexistent/manifest.json"), {})
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        broken = os.path.join(tmp.name, "manifest.json")
        with open(broken, "w", encoding="utf-8") as f:
            f.write("{not json")
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            self.assertEqual(rer.load_media(broken), {})
        self.assertIn("failed to parse media manifest", stderr.getvalue())


class JunitFallbackTest(unittest.TestCase):
    JUNIT_XML = """<testsuites>
      <testsuite name="smoke/x.feature.spec.js" hostname="smoke">
        <testcase name="Feature › TC-1 boots" classname="../../.features-gen/smoke/e2e/features/smoke/x.feature.spec.js"/>
        <testcase name="Feature › TC-9 broken" classname="../../.features-gen/smoke/e2e/features/smoke/x.feature.spec.js">
          <failure message="boom"/>
        </testcase>
      </testsuite>
    </testsuites>"""

    def test_rows_from_junit_when_no_allure(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        shard = os.path.join(tmp.name, "e2e-junit-smoke-linux")
        os.makedirs(shard)
        with open(os.path.join(shard, "junit.xml"), "w", encoding="utf-8") as f:
            f.write(self.JUNIT_XML)
        rows = rer.rows_from_junit(tmp.name)
        self.assertEqual(len(rows), 2)
        body = rer.render_pr_comment(rer.aggregate(rows), rows, [], {"launch": "", "run": ""}, flaky_known=False)
        self.assertIn("| — |", body)  # flaky column is a dash when only junit data exists

    def test_malformed_junit_diagnostics_go_to_stderr_not_stdout(self):
        # A broken junit.xml must not pollute stdout — in pr-comment mode
        # stdout IS the comment body and MARKER must stay the first line.
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        shard = os.path.join(tmp.name, "e2e-junit-smoke-linux")
        os.makedirs(shard)
        with open(os.path.join(shard, "junit.xml"), "w", encoding="utf-8") as f:
            f.write("<testsuites><unclosed")
        stdout = io.StringIO()
        stderr = io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            product, _, infra, _, oses = rer.collect_junit(tmp.name)
            rows = rer.rows_from_junit(tmp.name)
        self.assertEqual(stdout.getvalue(), "")
        self.assertIn("failed to parse", stderr.getvalue())
        self.assertEqual(dict(product), {})
        self.assertEqual(rows, [])
        self.assertEqual(infra, [])
        self.assertEqual(oses, [])


class StepSummaryTest(unittest.TestCase):
    def test_summary_tables_and_flaky_marker(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        junit_root = os.path.join(tmp.name, "junit")
        shard = os.path.join(junit_root, "e2e-junit-smoke-linux")
        os.makedirs(shard)
        with open(os.path.join(shard, "junit.xml"), "w", encoding="utf-8") as f:
            f.write(JunitFallbackTest.JUNIT_XML)
        allure_root = os.path.join(tmp.name, "allure")
        allure_shard = os.path.join(allure_root, "allure-results-smoke-linux")
        write_result(allure_shard, "TC-1 boots", "failed", 1000)
        write_result(allure_shard, "TC-1 boots", "passed", 2000)

        product, features, infra, tally, oses = rer.collect_junit(junit_root)
        flaky_names = rer.flaky_name_set(rer.finalize(rer.collect_allure(allure_root)[0]))
        text = rer.render_step_summary(product, features, infra, tally, oses, flaky_names)
        self.assertIn("## E2E Test Results", text)
        self.assertIn("### Coverage", text)
        self.assertIn("| Project | Linux |", text)
        # the flaky junit-passed test gets the marker; junit name's last segment matches allure name
        self.assertIn(f"{rer.PASS} {rer.FLAKY_MARK}", text)

    def test_empty_state(self):
        product, features, infra, tally, oses = rer.collect_junit("/nonexistent")
        text = rer.render_step_summary(product, features, infra, tally, oses or ["linux"], set())
        self.assertIn("No JUnit results found", text)


class MergeTest(unittest.TestCase):
    def test_merge_injects_missing_platform(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = os.path.join(tmp.name, "shards")
        shard = os.path.join(root, "allure-results-security-linux")
        write_result(shard, "no-platform attempt", "failed", 1000, project_param="security")
        # non-result files (attachments, containers) must be copied through untouched
        with open(os.path.join(shard, "blob-attachment.txt"), "w", encoding="utf-8") as f:
            f.write("trace")
        out = os.path.join(tmp.name, "merged")
        fixed = rer.merge_shards(root, out)
        self.assertEqual(fixed, 1)
        merged_results = [f for f in os.listdir(out) if f.endswith("-result.json")]
        self.assertEqual(len(merged_results), 1)
        with open(os.path.join(out, merged_results[0]), encoding="utf-8") as f:
            result = json.load(f)
        self.assertIn({"name": "Platform", "value": "Linux"}, result["parameters"])
        self.assertTrue(os.path.isfile(os.path.join(out, "blob-attachment.txt")))

    def test_merge_keeps_existing_platform(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = os.path.join(tmp.name, "shards")
        shard = os.path.join(root, "allure-results-security-macos")
        os.makedirs(shard)
        result = {"uuid": "u1", "name": "t", "status": "passed", "start": 1, "stop": 2,
                  "labels": [], "parameters": [{"name": "Platform", "value": "macOS"}]}
        with open(os.path.join(shard, "u1-result.json"), "w", encoding="utf-8") as f:
            json.dump(result, f)
        out = os.path.join(tmp.name, "merged")
        fixed = rer.merge_shards(root, out)
        self.assertEqual(fixed, 0)
        with open(os.path.join(out, "u1-result.json"), encoding="utf-8") as f:
            merged = json.load(f)
        self.assertEqual(merged["parameters"], [{"name": "Platform", "value": "macOS"}])


class AttachmentMediaTest(unittest.TestCase):
    def make_root(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        return self.tmp.name

    def test_attempts_carry_media_paths(self):
        root = self.make_root()
        shard = os.path.join(root, "allure-results-smoke-linux")
        write_result(shard, "TC-1 boots", "failed", 1000,
                     attachments=[SCREENSHOT_ATT, VIDEO_ATT])
        tests, _ = rer.collect_allure(root)
        attempts = tests[("smoke", "linux", "TC-1 boots")]["attempts"]
        self.assertEqual(len(attempts), 1)
        start, status, media = attempts[0]
        self.assertEqual((start, status), (1000, "failed"))
        self.assertEqual(media["screenshot"], os.path.join(shard, "s1-attachment.png"))
        self.assertEqual(media["video"], os.path.join(shard, "v1-attachment.webm"))

    def test_custom_named_screenshot_is_not_failure_media(self):
        # BDD steps attach deliberate screenshots under custom names.
        root = self.make_root()
        shard = os.path.join(root, "allure-results-smoke-linux")
        write_result(shard, "TC-2 renders", "passed", 1000, attachments=[
            {"name": "my-dashboard", "source": "d1-attachment.png", "type": "image/png"},
        ])
        tests, _ = rer.collect_allure(root)
        _, _, media = tests[("smoke", "linux", "TC-2 renders")]["attempts"][0]
        self.assertEqual(media, {})

    def test_video_matches_on_name_or_type(self):
        # Video match is deliberately permissive: name "video" (even with a
        # codec-suffixed type) OR type "video/webm" (under any name) counts.
        root = self.make_root()
        shard = os.path.join(root, "allure-results-smoke-linux")
        write_result(shard, "TC-4 by name", "failed", 1000, uuid_hint="n", attachments=[
            {"name": "video", "source": "v2-attachment.webm", "type": "video/webm;codecs=vp8"},
        ])
        write_result(shard, "TC-5 by type", "failed", 1000, uuid_hint="t", attachments=[
            {"name": "trace-video", "source": "v3-attachment.webm", "type": "video/webm"},
        ])
        tests, _ = rer.collect_allure(root)
        _, _, by_name = tests[("smoke", "linux", "TC-4 by name")]["attempts"][0]
        _, _, by_type = tests[("smoke", "linux", "TC-5 by type")]["attempts"][0]
        self.assertEqual(by_name["video"], os.path.join(shard, "v2-attachment.webm"))
        self.assertEqual(by_type["video"], os.path.join(shard, "v3-attachment.webm"))

    def test_step_nested_attachments_are_collected(self):
        # allure-playwright 3 records every testInfo.attach as an attachment
        # STEP: the file reference lives in steps[].attachments (nested 1-2
        # levels deep, e.g. under the teardown fixture step) — the result's
        # top-level attachments carry only writer-added stdout. This is the
        # shape real CI shards have; missing it left every PR-comment media
        # link empty (PR #747).
        root = self.make_root()
        shard = os.path.join(root, "allure-results-smoke-linux")
        write_result(shard, "TC-6 nested", "failed", 1000, attachments=[
            {"name": "stdout", "source": "o1-attachment.txt", "type": "text/plain"},
        ], steps=[
            {"name": "screenshot", "attachments": [SCREENSHOT_ATT]},
            {"name": "fixture: electronApp", "steps": [
                {"name": "video", "attachments": [VIDEO_ATT]},
                {"name": "my-dashboard", "attachments": [
                    {"name": "my-dashboard", "source": "d1-attachment.png", "type": "image/png"},
                ]},
            ]},
        ])
        tests, _ = rer.collect_allure(root)
        _, _, media = tests[("smoke", "linux", "TC-6 nested")]["attempts"][0]
        self.assertEqual(media["screenshot"], os.path.join(shard, "s1-attachment.png"))
        self.assertEqual(media["video"], os.path.join(shard, "v1-attachment.webm"))

    def test_duplicate_start_and_status_does_not_crash_finalize(self):
        # (start, status, dict) tuples must not be compared element-wise.
        root = self.make_root()
        shard = os.path.join(root, "allure-results-smoke-linux")
        write_result(shard, "TC-3 twin", "failed", 1000, uuid_hint="a",
                     attachments=[SCREENSHOT_ATT])
        write_result(shard, "TC-3 twin", "failed", 1000, uuid_hint="b",
                     attachments=[VIDEO_ATT])
        rows = rer.finalize(rer.collect_allure(root)[0])
        self.assertEqual(rows[0]["status"], "failed")


class FailureMediaTest(unittest.TestCase):
    def make_root(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        return self.tmp.name

    def out_dir(self):
        out = tempfile.TemporaryDirectory()
        self.addCleanup(out.cleanup)
        return out.name

    def test_failed_test_gets_screenshot_and_video_from_latest_attempt(self):
        root = self.make_root()
        shard = os.path.join(root, "allure-results-smoke-linux")
        write_attachment(shard, "s1-attachment.png", b"old")
        write_attachment(shard, "s2-attachment.png", b"new")
        write_attachment(shard, "v1-attachment.webm", b"vid")
        write_result(shard, "TC-1 boots", "failed", 1000, uuid_hint="a", attachments=[
            {"name": "screenshot", "source": "s1-attachment.png", "type": "image/png"},
        ])
        write_result(shard, "TC-1 boots", "failed", 2000, uuid_hint="b", attachments=[
            {"name": "screenshot", "source": "s2-attachment.png", "type": "image/png"},
            {"name": "video", "source": "v1-attachment.webm", "type": "video/webm"},
        ])
        out = self.out_dir()
        manifest = rer.write_failure_media(rer.collect_allure(root)[0], out)
        self.assertEqual(manifest, [{
            "project": "smoke", "os": "linux", "name": "TC-1 boots",
            "dir": "0", "files": ["screenshot.png", "video.webm"],
        }])
        with open(os.path.join(out, "0", "screenshot.png"), "rb") as f:
            self.assertEqual(f.read(), b"new")  # latest attempt's screenshot wins
        self.assertTrue(os.path.isfile(os.path.join(out, "0", "video.webm")))
        with open(os.path.join(out, "manifest.json"), encoding="utf-8") as f:
            self.assertEqual(json.load(f), manifest)

    def test_flaky_test_takes_screenshot_from_failed_attempt_video_from_retry(self):
        root = self.make_root()
        shard = os.path.join(root, "allure-results-smoke-linux")
        write_attachment(shard, "s1-attachment.png")
        write_attachment(shard, "v1-attachment.webm")
        write_result(shard, "TC-2 flaky", "failed", 1000, uuid_hint="a", attachments=[
            {"name": "screenshot", "source": "s1-attachment.png", "type": "image/png"},
        ])
        write_result(shard, "TC-2 flaky", "passed", 2000, uuid_hint="b", attachments=[
            {"name": "video", "source": "v1-attachment.webm", "type": "video/webm"},
        ])
        manifest = rer.write_failure_media(rer.collect_allure(root)[0], self.out_dir())
        self.assertEqual(manifest[0]["files"], ["screenshot.png", "video.webm"])

    def test_passed_and_mediafree_tests_are_excluded(self):
        root = self.make_root()
        shard = os.path.join(root, "allure-results-smoke-linux")
        write_attachment(shard, "s1-attachment.png")
        write_result(shard, "TC-3 green", "passed", 1000, attachments=[
            {"name": "screenshot", "source": "s1-attachment.png", "type": "image/png"},
        ])
        write_result(shard, "TC-4 bare fail", "failed", 1000)
        out = self.out_dir()
        manifest = rer.write_failure_media(rer.collect_allure(root)[0], out)
        self.assertEqual(manifest, [])
        with open(os.path.join(out, "manifest.json"), encoding="utf-8") as f:
            self.assertEqual(json.load(f), [])

    def test_missing_source_file_skipped_with_stderr_note(self):
        root = self.make_root()
        shard = os.path.join(root, "allure-results-smoke-linux")
        write_attachment(shard, "v1-attachment.webm")
        write_result(shard, "TC-5 gone", "failed", 1000, attachments=[
            {"name": "screenshot", "source": "missing-attachment.png", "type": "image/png"},
            {"name": "video", "source": "v1-attachment.webm", "type": "video/webm"},
        ])
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            manifest = rer.write_failure_media(rer.collect_allure(root)[0], self.out_dir())
        self.assertEqual(manifest[0]["files"], ["video.webm"])
        self.assertIn("media file missing", stderr.getvalue())

    def test_cap_prefers_failed_over_flaky(self):
        root = self.make_root()
        shard = os.path.join(root, "allure-results-smoke-linux")
        write_attachment(shard, "s1-attachment.png")
        att = [{"name": "screenshot", "source": "s1-attachment.png", "type": "image/png"}]
        write_result(shard, "TC-A flaky", "failed", 1000, uuid_hint="a", attachments=att)
        write_result(shard, "TC-A flaky", "passed", 2000, uuid_hint="b")
        write_result(shard, "TC-B fail", "failed", 1000, uuid_hint="b", attachments=att)
        write_result(shard, "TC-C fail", "failed", 1000, uuid_hint="c", attachments=att)
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            manifest = rer.write_failure_media(rer.collect_allure(root)[0], self.out_dir(), cap=2)
        names = [e["name"] for e in manifest]
        self.assertEqual(sorted(names), ["TC-B fail", "TC-C fail"])  # flaky dropped first
        self.assertIn("dropped 1", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
