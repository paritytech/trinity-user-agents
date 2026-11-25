#!/usr/bin/env python3
"""Measure the WebContent memory of ten WKWebView workers on a connected iPhone.

    scripts/measure-ios-webcontent.py <device-udid> <bounds|guard> [label]

The app cannot read another process's memory, so this launches the bench app
with `--suite webview-memory --hold 30`, waits for its `BENCH_HOLD` line,
records Instruments' Activity Monitor on the device while the ten WebViews are
held, and sums the memory of the WebContent processes the app named. Writes
results/ios-device/<model>-webview-memory-<label>.jsonl.
"""

import json
import os
import re
import subprocess
import sys
import xml.etree.ElementTree as ElementTree

HOLD_SECONDS = 30


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    device, variant = sys.argv[1], sys.argv[2]
    label = sys.argv[3] if len(sys.argv) > 3 else "run"
    os.chdir(os.path.join(os.path.dirname(__file__), ".."))
    app = f"build/xcode-{variant}/Release-iphoneos/QJSBench.app"
    subprocess.run(["xcrun", "devicectl", "device", "install", "app", "--device", device, app],
                   check=True, capture_output=True)

    launch = subprocess.Popen(
        ["xcrun", "devicectl", "device", "process", "launch", "--device", device, "--console",
         "--terminate-existing", "io.parity.qjsbench", "--suite", "webview-memory",
         "--hold", str(HOLD_SECONDS), "--exit"],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    pids: list[int] = []
    result_line = None
    trace = "build/webcontent.trace"
    for line in launch.stdout:
        if line.startswith("BENCH_HOLD"):
            pids = [int(pid) for pid in re.findall(r"\d+", line.split("pids=", 1)[1])]
            subprocess.run(["rm", "-rf", trace])
            subprocess.run(["xcrun", "xctrace", "record", "--template", "Activity Monitor", "--device", device,
                            "--all-processes", "--time-limit", "8s", "--output", trace],
                           check=True, capture_output=True)
        elif line.startswith("BENCH ") and '"mode":"instances"' in line:
            result_line = line.removeprefix("BENCH ").strip()
    launch.wait()
    if not pids or result_line is None:
        print("the app did not hold the WebViews", file=sys.stderr)
        return 1

    memory = process_memory(trace)
    content = [memory[pid] for pid in pids if pid in memory]
    record = json.loads(result_line)
    record["result"]["webContentMiBTotal"] = round(sum(content) / 1_048_576, 2)
    record["result"]["webContentMeasured"] = len(content)
    record["result"]["webContentSource"] = "xctrace Activity Monitor, physical footprint"
    out_dir = "results/ios-device"
    os.makedirs(out_dir, exist_ok=True)
    out_path = f"{out_dir}/{record['model']}-webview-memory-{label}.jsonl"
    with open(out_path, "w") as out:
        out.write(json.dumps(record, separators=(",", ":")) + "\n")
    print(json.dumps(record["result"]))
    print(f"wrote {out_path}")
    return 0


def process_memory(trace: str) -> dict[int, int]:
    """The last memory sample in bytes per pid, from the trace's live-processes table."""
    exported = subprocess.run(
        ["xcrun", "xctrace", "export", "--input", trace, "--xpath",
         '/trace-toc/run/data/table[@schema="activity-monitor-process-live"]'],
        check=True, capture_output=True, text=True).stdout
    root = ElementTree.fromstring(exported)
    columns = [mnemonic.text for mnemonic in root.iter("mnemonic")]
    by_id: dict[str, ElementTree.Element] = {}
    for element in root.iter():
        if "id" in element.attrib:
            by_id[element.attrib["id"]] = element

    def resolve(element: ElementTree.Element) -> ElementTree.Element:
        return by_id[element.attrib["ref"]] if "ref" in element.attrib else element

    memory: dict[int, int] = {}
    for row in root.iter("row"):
        values = dict(zip(columns, (resolve(cell) for cell in row)))
        pid = values.get("pid")
        footprint = values.get("memory-physical-footprint")
        if pid is not None and footprint is not None and pid.text and footprint.text:
            memory[int(pid.text)] = int(footprint.text)
    return memory


if __name__ == "__main__":
    sys.exit(main())
