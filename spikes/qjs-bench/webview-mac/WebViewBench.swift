// E1 on macOS: the same workload files and corpus in a hidden WKWebView,
// set up the way WKWebViewJSEngine does it on iOS. JavaScript runs in a
// separate WebContent process with the JIT, as on iOS.
//
//   webview-bench instances <shim.js> <w-worker.js> <count>
//   webview-bench replay    <shim.js> <w-worker.js> <corpus.bin>
//   webview-bench micro     <shim.js> <w-codec.js>
//
// Prints one `RESULT {json}` line.

import Darwin
import Foundation
import WebKit

func footprintBytes(_ pid: pid_t) -> UInt64 {
    var info = rusage_info_v2()
    let status = withUnsafeMutablePointer(to: &info) { pointer in
        pointer.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) { proc_pid_rusage(pid, RUSAGE_INFO_V2, $0) }
    }
    return status == 0 ? info.ri_phys_footprint : 0
}

func mib(_ bytes: UInt64) -> Double { Double(bytes) / 1_048_576 }

func percentiles(_ samples: [Double]) -> (Double, Double) {
    let sorted = samples.sorted()
    let at = { (fraction: Double) in sorted[min(sorted.count - 1, Int(Double(sorted.count) * fraction))] }
    return (at(0.5), at(0.95))
}

struct Step {
    let inbound: [UInt8]
    let outbound: [[UInt8]]
}

func loadCorpus(_ path: String) -> [Step] {
    let bytes = [UInt8](FileManager.default.contents(atPath: path)!)
    var offset = 0
    func take(_ len: Int) -> [UInt8] {
        defer { offset += len }
        return Array(bytes[offset..<offset + len])
    }
    func u32() -> Int {
        let value = take(4)
        return Int(value[0]) | Int(value[1]) << 8 | Int(value[2]) << 16 | Int(value[3]) << 24
    }
    precondition(String(decoding: take(4), as: UTF8.self) == "QJSC")
    precondition(u32() == 1)
    return (0..<u32()).map { _ in
        let inbound = take(u32())
        let outbound = (0..<u32()).map { _ in take(u32()) }
        return Step(inbound: inbound, outbound: outbound)
    }
}

func isRenderStart(_ frame: [UInt8]) -> Bool {
    guard let first = frame.first, first & 0b11 == 0 else { return false }
    let header = 1 + Int(first >> 2)
    return frame.count > header + 2 && frame[header] == 17 && frame[header + 1] == 0 && frame[header + 2] == 0
}

@MainActor
final class Bench: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
    let webView: WKWebView
    private var loaded: CheckedContinuation<Void, Never>?
    private var waitingFrames: (count: Int, continuation: CheckedContinuation<Void, Never>)?
    private var waitingResult: CheckedContinuation<String, Never>?
    var frames: [[UInt8]] = []

    init(shim: String) {
        let configuration = WKWebViewConfiguration()
        let controller = WKUserContentController()
        controller.addUserScript(WKUserScript(source: shim, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        configuration.userContentController = controller
        webView = WKWebView(frame: CGRect(x: 0, y: 0, width: 1, height: 1), configuration: configuration)
        super.init()
        controller.add(self, name: "truapiFrame")
        controller.add(self, name: "truapiLog")
        webView.navigationDelegate = self
    }

    var webContentPid: pid_t { (webView.value(forKey: "_webProcessIdentifier") as? NSNumber)?.int32Value ?? 0 }

    func load() async {
        await withCheckedContinuation { continuation in
            loaded = continuation
            webView.loadHTMLString("<!doctype html><html><body></body></html>", baseURL: URL(string: "https://bench.local/"))
        }
    }

    func evaluate(_ source: String) async {
        _ = try? await webView.evaluateJavaScript(source)
    }

    /// Deliver one frame and wait until `expected` frames have come back.
    func deliver(_ frame: [UInt8], expecting expected: Int) async {
        let call = "__truapi_deliver_bytes([\(frame.map(String.init).joined(separator: ","))]);0"
        await withCheckedContinuation { continuation in
            waitingFrames = (expected, continuation)
            webView.evaluateJavaScript(call, completionHandler: nil)
            resumeIfFramesArrived()
        }
    }

    func awaitResult() async -> String {
        await withCheckedContinuation { waitingResult = $0 }
    }

    private func resumeIfFramesArrived() {
        if let waiting = waitingFrames, frames.count >= waiting.count {
            waitingFrames = nil
            waiting.continuation.resume()
        }
    }

    nonisolated func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        MainActor.assumeIsolated {
            loaded?.resume()
            loaded = nil
        }
    }

    nonisolated func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        MainActor.assumeIsolated {
            if message.name == "truapiFrame", let bytes = message.body as? [NSNumber] {
                frames.append(bytes.map(\.uint8Value))
                resumeIfFramesArrived()
            } else if message.name == "truapiLog", let body = message.body as? [String: Any],
                      let text = body["message"] as? String, text.hasPrefix("BENCH_RESULT ") {
                waitingResult?.resume(returning: String(text.dropFirst("BENCH_RESULT ".count)))
                waitingResult = nil
            }
        }
    }
}

@MainActor
func boot(shim: String, workload: String) async -> (Bench, Double) {
    let started = Date()
    let bench = Bench(shim: shim)
    await bench.load()
    await bench.evaluate(workload)
    await bench.evaluate("0")
    return (bench, Date().timeIntervalSince(started) * 1000)
}

@MainActor
func run() async {
    let args = CommandLine.arguments
    let mode = args[1]
    let shim = try! String(contentsOfFile: args[2], encoding: .utf8)
    let workload = try! String(contentsOfFile: args[3], encoding: .utf8)
    let ownPid = getpid()

    switch mode {
    case "instances":
        let count = Int(args[4])!
        let ownBefore = footprintBytes(ownPid)
        var benches: [Bench] = []
        var bootMs: [Double] = []
        var contentMiB: [Double] = []
        for _ in 0..<count {
            let (bench, elapsed) = await boot(shim: shim, workload: workload)
            benches.append(bench)
            bootMs.append(elapsed)
        }
        try? await Task.sleep(nanoseconds: 500_000_000)
        let pids = Set(benches.map(\.webContentPid))
        for pid in pids { contentMiB.append(mib(footprintBytes(pid))) }
        let ownGrowth = mib(footprintBytes(ownPid) - ownBefore)
        let (warmP50, _) = percentiles(Array(bootMs.dropFirst()))
        let contentTotal = contentMiB.reduce(0, +)
        print("RESULT {\"engine\":\"wkwebview\",\"firstBootMs\":\(String(format: "%.1f", bootMs[0])),\"bootP50Ms\":\(String(format: "%.1f", warmP50)),\"webContentProcesses\":\(pids.count),\"webContentMiBTotal\":\(String(format: "%.2f", contentTotal)),\"perInstanceMiB\":\(String(format: "%.2f", (contentTotal + ownGrowth) / Double(count))),\"appProcessGrowthMiB\":\(String(format: "%.2f", ownGrowth)),\"instances\":\(count)}")
    case "replay":
        let steps = loadCorpus(args[4])
        let (bench, bootMs) = await boot(shim: shim, workload: workload)
        var renderMs: [Double] = []
        var mismatches = 0
        for step in steps {
            bench.frames = []
            let started = Date()
            await bench.deliver(step.inbound, expecting: step.outbound.count)
            if step.outbound.isEmpty { await bench.evaluate("0") }
            let elapsed = Date().timeIntervalSince(started) * 1000
            if bench.frames != step.outbound { mismatches += 1 }
            if isRenderStart(step.inbound) { renderMs.append(elapsed) }
        }
        let (p50, p95) = percentiles(renderMs)
        print("RESULT {\"engine\":\"wkwebview\",\"bootMs\":\(String(format: "%.1f", bootMs)),\"steps\":\(steps.count),\"mismatches\":\(mismatches),\"renderSamples\":\(renderMs.count),\"renderP50Ms\":\(String(format: "%.3f", p50)),\"renderP95Ms\":\(String(format: "%.3f", p95)),\"webContentMiB\":\(String(format: "%.2f", mib(footprintBytes(bench.webContentPid))))}")
    default:
        let bench = Bench(shim: shim)
        await bench.load()
        async let result = bench.awaitResult()
        await bench.evaluate(workload)
        print("RESULT \(await result)")
    }
    exit(0)
}

Task { @MainActor in await run() }
RunLoop.main.run()
