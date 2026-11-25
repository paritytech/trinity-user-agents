import BenchKit
import Foundation
import UIKit

/// One measurement: an engine, a mode and, for `instances` and
/// `guard-capacity`, a count.
struct BenchStep {
    let engine: String
    let mode: String
    var count: UInt32 = 0
}

@MainActor
final class BenchRunner: ObservableObject {
    @Published private(set) var lines: [String] = []
    @Published private(set) var status = "Starting"

    private let arguments = ProcessInfo.processInfo.arguments
    private var started = false

    func start() async {
        guard !started else { return }
        started = true
        UIApplication.shared.isIdleTimerDisabled = true

        let variant = String(cString: benchkit_variant())
        let steps = makeSuite(named: argument("--suite") ?? "quick", runs: Int(argument("--runs") ?? "") ?? 3)
        let corpus = Bundle.main.path(forResource: "corpus", ofType: "bin")
        let resources = corpus.map { ($0 as NSString).deletingLastPathComponent } ?? Bundle.main.resourcePath ?? ""
        emit("{\"suite\":\"\(argument("--suite") ?? "quick")\",\"variant\":\"\(variant)\",\(deviceFields())}")

        for (index, step) in steps.enumerated() {
            status = "\(index + 1)/\(steps.count) \(step.engine) \(step.mode)"
            let line = step.engine == "webview"
                ? await WebViewBench.run(mode: step.mode, resources: resources, holdSeconds: Int(argument("--hold") ?? "") ?? 0)
                : await Task.detached(priority: .userInitiated) {
                    Self.run(step, resources: resources)
                }.value
            emit("{\(deviceFields()),\"step\":\(index)," + line.dropFirst())
        }

        status = "Done, \(steps.count) steps"
        emit("{\"done\":true,\"steps\":\(steps.count)}")
        if arguments.contains("--exit") {
            try? await Task.sleep(for: .milliseconds(300))
            exit(0)
        }
    }

    private nonisolated static func run(_ step: BenchStep, resources: String) -> String {
        guard let raw = benchkit_run(step.engine, step.mode, resources, step.count) else {
            return "{\"ok\":false,\"error\":\"benchkit returned nothing\"}"
        }
        defer { benchkit_free(raw) }
        return String(cString: raw)
    }

    private func makeSuite(named suite: String, runs: Int) -> [BenchStep] {
        if suite == "webview-memory" {
            return [BenchStep(engine: "webview", mode: "instances", count: 10)]
        }
        let engines = ["native", "wasm2c", "boa", "wasmi"]
        var steps = [BenchStep(engine: "wasm2c", mode: "guard-capacity", count: 64)]
        for engine in engines {
            steps += Array(repeating: BenchStep(engine: engine, mode: "replay"), count: runs)
            steps.append(BenchStep(engine: engine, mode: "instances", count: 10))
        }
        steps += Array(repeating: BenchStep(engine: "webview", mode: "replay"), count: runs)
        steps.append(BenchStep(engine: "webview", mode: "instances", count: 10))
        steps.append(BenchStep(engine: "wasm2c", mode: "memcap"))
        steps.append(BenchStep(engine: "wasmi", mode: "memcap"))
        steps.append(BenchStep(engine: "wasm2c", mode: "oob-trap"))
        let workloads = suite == "full" ? ["codec", "rxjs", "pingpong-64", "pingpong-4k"] : ["codec"]
        let throughputEngines = suite == "full" ? engines : ["native", "wasm2c", "boa"]
        for workload in workloads {
            for engine in throughputEngines {
                steps.append(BenchStep(engine: engine, mode: workload))
            }
            // The ping-pong workloads wait on echoed frames the WebView shim never sends back.
            if !workload.hasPrefix("pingpong") {
                steps.append(BenchStep(engine: "webview", mode: workload))
            }
        }
        return steps
    }

    private func emit(_ line: String) {
        lines.append(line)
        print("BENCH \(line)")
        fflush(stdout)
        append(line)
    }

    private func append(_ line: String) {
        guard let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first else {
            return
        }
        let file = documents.appendingPathComponent("results.jsonl")
        let data = Data((line + "\n").utf8)
        if let handle = try? FileHandle(forWritingTo: file) {
            handle.seekToEndOfFile()
            handle.write(data)
            try? handle.close()
        } else {
            try? data.write(to: file)
        }
    }

    private func argument(_ name: String) -> String? {
        guard let index = arguments.firstIndex(of: name), index + 1 < arguments.count else { return nil }
        return arguments[index + 1]
    }

    private func deviceFields() -> String {
        let device = UIDevice.current
        return "\"model\":\"\(Self.modelIdentifier())\",\"os\":\"\(device.systemVersion)\","
            + "\"thermal\":\"\(Self.thermalState())\",\"lowPower\":\(ProcessInfo.processInfo.isLowPowerModeEnabled)"
    }

    private static func modelIdentifier() -> String {
        if let simulated = ProcessInfo.processInfo.environment["SIMULATOR_MODEL_IDENTIFIER"] {
            return "simulator:\(simulated)"
        }
        var system = utsname()
        uname(&system)
        return withUnsafeBytes(of: &system.machine) { buffer in
            String(decoding: buffer.prefix(while: { $0 != 0 }), as: UTF8.self)
        }
    }

    private static func thermalState() -> String {
        switch ProcessInfo.processInfo.thermalState {
        case .nominal: "nominal"
        case .fair: "fair"
        case .serious: "serious"
        case .critical: "critical"
        @unknown default: "unknown"
        }
    }
}
