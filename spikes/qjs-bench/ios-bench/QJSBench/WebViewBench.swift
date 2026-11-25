import Darwin
import Foundation
import UIKit
import WebKit

/// The same workload in a WKWebView, as the iOS host runs workers today.
///
/// Frames leave through the `truapiFrame` message handler and come back
/// through `__truapi_deliver_bytes`, with the shim the Mac bench uses. The
/// view is 1 pt and attached to the window, so WebKit does not throttle it as
/// hidden. JavaScript runs in a separate WebContent process with the JIT.
@MainActor
final class WebViewBench: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
    private let webView: WKWebView
    private var loaded: CheckedContinuation<Void, Never>?
    private var waitingFrames: (count: Int, continuation: CheckedContinuation<Void, Never>)?
    private var waitingResult: CheckedContinuation<String, Never>?
    private(set) var frames: [[UInt8]] = []

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
        Self.window?.addSubview(webView)
    }

    /// The WebContent process's pid, through WebKit's private accessor.
    var webContentPid: pid_t { (webView.value(forKey: "_webProcessIdentifier") as? NSNumber)?.int32Value ?? 0 }

    /// Load a blank page, then the workload, and return the elapsed milliseconds.
    func boot(workload: String) async -> Double {
        let started = Date()
        await withCheckedContinuation { continuation in
            loaded = continuation
            webView.loadHTMLString("<!doctype html><html><body></body></html>", baseURL: URL(string: "https://bench.local/"))
        }
        await evaluate(workload)
        await evaluate("0")
        return Date().timeIntervalSince(started) * 1000
    }

    func evaluate(_ source: String) async {
        _ = try? await webView.evaluateJavaScript(source)
    }

    /// Deliver one frame and wait until `expected` frames have come back.
    func deliver(_ frame: [UInt8], expecting expected: Int) async -> [[UInt8]] {
        frames = []
        let call = "__truapi_deliver_bytes([\(frame.map(String.init).joined(separator: ","))]);0"
        await withCheckedContinuation { continuation in
            waitingFrames = (expected, continuation)
            webView.evaluateJavaScript(call, completionHandler: nil)
            resumeIfFramesArrived()
        }
        if expected == 0 { await evaluate("0") }
        return frames
    }

    /// Run a throughput workload to its `BENCH_RESULT` line.
    func runToResult(workload: String) async -> String {
        _ = await boot(workload: "0")
        return await withCheckedContinuation { continuation in
            waitingResult = continuation
            webView.evaluateJavaScript(workload, completionHandler: nil)
        }
    }

    func destroy() {
        webView.removeFromSuperview()
        webView.configuration.userContentController.removeAllScriptMessageHandlers()
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

    private static var window: UIWindow? {
        UIApplication.shared.connectedScenes.compactMap { ($0 as? UIWindowScene)?.keyWindow }.first
    }
}

extension WebViewBench {
    /// Run one WebView step and return its result line.
    static func run(mode: String, resources: String, holdSeconds: Int) async -> String {
        let head = "\"engine\":\"webview\",\"mode\":\"\(mode)\",\"variant\":\"wkwebview\""
        func read(_ name: String) -> String {
            (try? String(contentsOfFile: "\(resources)/\(name)", encoding: .utf8)) ?? ""
        }
        let shim = read("webview-shim.js")
        switch mode {
        case "replay":
            let steps = loadCorpus("\(resources)/corpus.bin")
            let bench = WebViewBench(shim: shim)
            let bootMs = await bench.boot(workload: read("w-worker.js"))
            var renderMs: [Double] = []
            var mismatches = 0
            for step in steps {
                let started = Date()
                let sent = await bench.deliver(step.inbound, expecting: step.outbound.count)
                let elapsed = Date().timeIntervalSince(started) * 1000
                if sent != step.outbound { mismatches += 1 }
                if isRenderStart(step.inbound) { renderMs.append(elapsed) }
            }
            bench.destroy()
            let (p50, p95) = percentiles(renderMs)
            return "{\(head),\"ok\":true,\"result\":{\"engine\":\"webview\",\"bootMs\":\(format(bootMs, 1)),\"steps\":\(steps.count),\"mismatches\":\(mismatches),\"renderSamples\":\(renderMs.count),\"renderP50Ms\":\(format(p50, 3)),\"renderP95Ms\":\(format(p95, 3))}}"
        case "instances":
            let count = 10
            let before = ownFootprint()
            var benches: [WebViewBench] = []
            var bootMs: [Double] = []
            for _ in 0..<count {
                let bench = WebViewBench(shim: shim)
                bootMs.append(await bench.boot(workload: read("w-worker.js")))
                benches.append(bench)
            }
            try? await Task.sleep(for: .milliseconds(500))
            let growthMiB = Double(Int64(ownFootprint()) - Int64(before)) / 1_048_576
            let pids = Set(benches.map(\.webContentPid))
            let contentMiB = pids.map(footprint(of:))
            let readable = contentMiB.allSatisfy { $0 != nil }
            let contentTotal = contentMiB.compactMap { $0 }.reduce(0, +)
            // The sandbox hides WebContent memory from the app; hold the views so
            // scripts/measure-ios-webcontent.sh can record it with Instruments.
            if holdSeconds > 0 {
                print("BENCH_HOLD webview-instances pids=\(pids.sorted())")
                fflush(stdout)
                try? await Task.sleep(for: .seconds(holdSeconds))
            }
            benches.forEach { $0.destroy() }
            let (warmP50, _) = percentiles(Array(bootMs.dropFirst()))
            let content = readable ? format(contentTotal, 2) : "null"
            return "{\(head),\"ok\":true,\"result\":{\"engine\":\"webview\",\"firstBootMs\":\(format(bootMs[0], 1)),\"bootP50Ms\":\(format(warmP50, 1)),\"appProcessGrowthMiB\":\(format(growthMiB, 2)),\"webContentProcesses\":\(pids.count),\"webContentMiBTotal\":\(content),\"instances\":\(count)}}"
        default:
            let bench = WebViewBench(shim: shim)
            let result = await bench.runToResult(workload: read("w-\(mode).js"))
            bench.destroy()
            return "{\(head),\"ok\":true,\"result\":\(result)}"
        }
    }

    private static func format(_ value: Double, _ digits: Int) -> String {
        String(format: "%.\(digits)f", value)
    }

    private static func ownFootprint() -> UInt64 {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
        let status = withUnsafeMutablePointer(to: &info) { pointer in
            pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        return status == KERN_SUCCESS ? info.phys_footprint : 0
    }

    /// Another process's footprint in MiB, or `nil` when the sandbox refuses it.
    private static func footprint(of pid: pid_t) -> Double? {
        // rusage_info_v2: a 16-byte uuid, then u64 fields; ri_phys_footprint is the eighth.
        var buffer = [UInt64](repeating: 0, count: 64)
        let status = buffer.withUnsafeMutableBytes { proc_pid_rusage(pid, 2, $0.baseAddress!) }
        return status == 0 ? Double(buffer[2 + 7]) / 1_048_576 : nil
    }
}

/// libproc's `proc_pid_rusage`, which the iOS SDK does not declare.
@_silgen_name("proc_pid_rusage")
private func proc_pid_rusage(_ pid: Int32, _ flavor: Int32, _ buffer: UnsafeMutableRawPointer) -> Int32

/// One inbound frame and the frames the worker must answer it with.
struct CorpusStep {
    let inbound: [UInt8]
    let outbound: [[UInt8]]
}

/// `corpus.bin` as written by workloads/src/corpus-format.ts.
func loadCorpus(_ path: String) -> [CorpusStep] {
    let bytes = [UInt8](FileManager.default.contents(atPath: path) ?? Data())
    var offset = 0
    func take(_ length: Int) -> [UInt8] {
        defer { offset += length }
        return Array(bytes[offset..<offset + length])
    }
    func u32() -> Int {
        let value = take(4)
        return Int(value[0]) | Int(value[1]) << 8 | Int(value[2]) << 16 | Int(value[3]) << 24
    }
    precondition(String(decoding: take(4), as: UTF8.self) == "QJSC", "not a QJSC corpus")
    precondition(u32() == 1, "unsupported corpus version")
    return (0..<u32()).map { _ in
        let inbound = take(u32())
        let outbound = (0..<u32()).map { _ in take(u32()) }
        return CorpusStep(inbound: inbound, outbound: outbound)
    }
}

/// Whether `frame` is the host starting a render: trait 17, method 0, Start.
func isRenderStart(_ frame: [UInt8]) -> Bool {
    guard let first = frame.first, first & 0b11 == 0 else { return false }
    let header = 1 + Int(first >> 2)
    return frame.count > header + 2 && frame[header] == 17 && frame[header + 1] == 0 && frame[header + 2] == 0
}

/// `(p50, p95)` of the samples.
func percentiles(_ samples: [Double]) -> (Double, Double) {
    let sorted = samples.sorted()
    let at = { (fraction: Double) in sorted[min(sorted.count - 1, Int(Double(sorted.count) * fraction))] }
    return (at(0.5), at(0.95))
}
