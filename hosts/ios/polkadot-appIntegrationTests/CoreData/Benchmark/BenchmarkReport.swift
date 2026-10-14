import Foundation
import Testing

/// One JSON document per scenario and variant: attached to the test result and, when
/// `COREDATA_BENCH_OUT` names a directory, written there as `<scenario>.<variant>.json`.
struct BenchmarkReport: Codable {
    static let operationIOSVersion = "3.0.0-653010d"
    static let outputDirectoryEnvironmentKey = "COREDATA_BENCH_OUT"

    let scenario: String
    let variant: String
    let date: Date
    let operationIOSVersion: String
    let scale: BenchmarkScale
    let summaries: [LatencySummary]
    let counters: [String: Int]

    init(
        scenario: String,
        variant: StackVariant,
        scale: BenchmarkScale,
        summaries: [LatencySummary],
        counters: [String: Int] = [:]
    ) {
        self.scenario = scenario
        self.variant = variant.rawValue
        date = Date()
        operationIOSVersion = Self.operationIOSVersion
        self.scale = scale
        self.summaries = summaries
        self.counters = counters
    }

    static func publish(_ report: BenchmarkReport) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        encoder.dateEncodingStrategy = .iso8601

        let data = try encoder.encode(report)
        let fileName = "\(report.scenario).\(report.variant).json"

        Attachment.record(data, named: fileName)
        printSummary(report)

        if let directory = ProcessInfo.processInfo.environment[outputDirectoryEnvironmentKey] {
            let directoryURL = URL(fileURLWithPath: directory, isDirectory: true)
            try FileManager.default.createDirectory(at: directoryURL, withIntermediateDirectories: true)
            try data.write(to: directoryURL.appendingPathComponent(fileName))
        }
    }
}

private extension BenchmarkReport {
    static func printSummary(_ report: BenchmarkReport) {
        for summary in report.summaries {
            let line = String(
                format: "[bench] %@ %@ n=%d p50=%.2fms p95=%.2fms p99=%.2fms max=%.2fms ops/s=%.1f",
                summary.scenario,
                summary.variant,
                summary.count,
                summary.p50Ms,
                summary.p95Ms,
                summary.p99Ms,
                summary.maxMs,
                summary.opsPerSecond
            )
            print(line)
        }

        for (name, value) in report.counters.sorted(by: { $0.key < $1.key }) {
            print("[bench] \(report.scenario) \(report.variant) \(name)=\(value)")
        }
    }
}
