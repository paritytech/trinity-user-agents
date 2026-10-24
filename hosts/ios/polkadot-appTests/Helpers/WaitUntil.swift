import Foundation

struct WaitTimedOut: Error {}

/// Polls `condition` for work that finishes on a task the test cannot await.
@MainActor
func waitUntil(_ condition: @MainActor () -> Bool) async throws {
    for _ in 0 ..< 200 where !condition() {
        try await Task.sleep(for: .milliseconds(10))
    }
    guard condition() else {
        throw WaitTimedOut()
    }
}
