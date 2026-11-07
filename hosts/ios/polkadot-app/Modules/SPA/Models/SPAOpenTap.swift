import Foundation
import IssueMonitoring
import os

/// The tap that asked for a page, shared by every runtime the page builds. Each runtime has its own host
/// session; only the first one is handed the tap, once, and only while the tap is fresh, so a retry is
/// not timed from the original tap.
final class SPAOpenTap: Sendable {
    static let maximumAgeMilliseconds = 5 * 60 * 1_000

    private struct State {
        var consumed = false
        var pageSession: UUID?
    }

    let tap: HostOpenTap
    private let state = OSAllocatedUnfairLock(initialState: State())

    init(_ tap: HostOpenTap) {
        self.tap = tap
    }

    /// The session of the runtime the page runs now.
    var pageSession: UUID? {
        state.withLock { $0.pageSession }
    }

    func consume(bySession session: UUID, at now: Int) -> HostOpenTap? {
        let fresh = now - tap.monotonicMilliseconds <= Self.maximumAgeMilliseconds
        let first = state.withLock { state -> Bool in
            state.pageSession = session
            defer { state.consumed = true }
            return !state.consumed
        }

        return first && fresh ? tap : nil
    }
}
