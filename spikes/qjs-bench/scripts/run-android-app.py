#!/usr/bin/env python3
"""Run the Android bench app on a connected phone and keep its result lines.

    ADB=<adb> scripts/run-android-app.py <serial> <bounds|guard> [quick|full] [label]

Installs build/android-app/qjsbench-<variant>.apk, launches it, and follows
its `BENCH` lines in logcat into results/android-app/<model>-<variant>-<label>.jsonl.
When the app logs `BENCH_HOLD`, it is holding ten WebViews: this script then
adds the memory of the WebView renderer processes that appeared since launch,
which the app cannot read itself, as a `renderer-memory` line.
"""

import json
import os
import re
import subprocess
import sys
import time

PACKAGE = "io.parity.qjsbench"
ADB = os.environ.get("ADB", "adb")


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    serial, variant = sys.argv[1], sys.argv[2]
    suite = sys.argv[3] if len(sys.argv) > 3 else "quick"
    label = sys.argv[4] if len(sys.argv) > 4 else "run"
    os.chdir(os.path.join(os.path.dirname(__file__), ".."))

    def adb(*args: str) -> str:
        return subprocess.run([ADB, "-s", serial, *args], capture_output=True, text=True).stdout

    apk = f"build/android-app/qjsbench-{variant}.apk"
    model = adb("shell", "getprop", "ro.product.model").strip()
    out_dir = "results/android-app"
    os.makedirs(out_dir, exist_ok=True)
    out_path = f"{out_dir}/{model}-{variant}-{label}.jsonl"

    print(adb("install", "-r", apk).strip())
    adb("shell", "am", "force-stop", PACKAGE)
    renderers_before = renderer_pids(adb)
    adb("logcat", "-c")
    adb("shell", "am", "start", "-n", f"{PACKAGE}/.MainActivity",
        "--es", "suite", suite, "--ei", "runs", "3", "--ez", "exit", "true")

    seen = 0
    deadline = time.time() + 3600
    with open(out_path, "w") as out:
        while time.time() < deadline:
            time.sleep(1)
            lines = adb("logcat", "-d", "-v", "raw", "-s", "QJSBench:I").splitlines()
            for line in lines[seen:]:
                if line.startswith("BENCH_HOLD"):
                    record(out, renderer_memory(adb, renderers_before))
                elif line.startswith("BENCH "):
                    record(out, line.removeprefix("BENCH "))
                    if '"done":true' in line:
                        print(f"wrote {out_path}")
                        return 0
            seen = len(lines)
            if not adb("shell", "pidof", PACKAGE).strip() and seen and '"done":true' not in "".join(lines):
                time.sleep(2)
                if not adb("shell", "pidof", PACKAGE).strip():
                    print("the app died before finishing; crash log:", file=sys.stderr)
                    print(adb("logcat", "-d", "-b", "crash"), file=sys.stderr)
                    return 1
    print("timed out", file=sys.stderr)
    return 1


def record(out, line: str) -> None:
    print(line)
    out.write(line + "\n")
    out.flush()


def renderer_pids(adb) -> set[str]:
    listing = adb("shell", "ps", "-A", "-o", "PID,NAME")
    return {row.split()[0] for row in listing.splitlines() if "sandboxed_process" in row}


def renderer_memory(adb, before: set[str]) -> str:
    pids = sorted(renderer_pids(adb) - before)
    total_kib = 0
    for pid in pids:
        meminfo = adb("shell", "dumpsys", "meminfo", pid)
        match = re.search(r"TOTAL PSS:\s+(\d+)", meminfo) or re.search(r"^\s*TOTAL\s+(\d+)", meminfo, re.M)
        if match:
            total_kib += int(match.group(1))
    return json.dumps({
        "engine": "webview",
        "mode": "renderer-memory",
        "ok": True,
        "result": {"rendererProcesses": len(pids), "rendererMiB": round(total_kib / 1024, 2)},
    }, separators=(",", ":"))


if __name__ == "__main__":
    sys.exit(main())
