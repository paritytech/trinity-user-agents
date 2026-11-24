import SwiftUI
import UIKit

/// Runs the bench suite on launch and shows each result line.
///
/// Launch arguments: `--suite quick|full` (default `quick`), `--runs N`
/// (replays per engine, default 3) and `--exit` to quit when done, which is
/// how the scripts drive it. Results are printed to stdout and appended to
/// `Documents/results.jsonl`.
@main
struct QJSBenchApp: App {
    @StateObject private var runner = BenchRunner()

    var body: some Scene {
        WindowGroup {
            ResultsView(runner: runner)
                .task { await runner.start() }
        }
    }
}

struct ResultsView: View {
    @ObservedObject var runner: BenchRunner

    var body: some View {
        NavigationStack {
            List(Array(runner.lines.enumerated()), id: \.offset) { _, line in
                Text(line).font(.system(.caption2, design: .monospaced))
            }
            .navigationTitle(runner.status)
        }
    }
}
