import Foundation
import os
import TrUAPIHost

/// What the core last pushed about funding: each session with its progress,
/// what each overlay offers, and the history it recorded. The core sends each
/// with every change, and again when funding resumes, so the screens read
/// them here rather than from the core.
final class FundingSessionBook: Sendable {
    static let shared = FundingSessionBook()

    struct Entry: Sendable {
        var session: FundingSession
        let progress: FundingProgress?
    }

    private struct State: Sendable {
        var entries: [String: Entry] = [:]
        var options: [String: FundingOptions] = [:]
        var history: [FundingSessionSummary]?
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    func session(intent: String) -> FundingSession? {
        state.withLock { $0.entries[intent]?.session }
    }

    func progress(intent: String) -> FundingProgress? {
        state.withLock { $0.entries[intent]?.progress }
    }

    /// Whether the provider's screen has done its part: the payment is seen
    /// or the session is over, which is when the frame closes itself.
    func paymentMoved(intent: String) -> Bool {
        guard let session = session(intent: intent), session.stage.isOpen else { return true }
        return progress(intent: intent)?.steps.contains { $0.reachedAtMs != nil && $0.step != .started } ?? false
    }

    /// The sessions the core still hands over: those in flight, and ended
    /// ones the host has not recorded.
    func unrecorded() -> [Entry] {
        state.withLock { $0.entries.values.filter { !$0.session.acknowledged } }
    }

    /// Keeps the session as the core holds it, or drops it once the core
    /// keeps only its summary.
    func record(intent: String, session: FundingSession?, progress: FundingProgress?) {
        state.withLock { state in
            if let session {
                state.entries[intent] = Entry(session: session, progress: progress)
            } else {
                state.entries[intent] = nil
                state.options[intent] = nil
            }
        }
    }

    /// The host wrote ended session `intent` into its own history.
    func markRecorded(intent: String) {
        state.withLock { $0.entries[intent]?.session.acknowledged = true }
    }

    func options(intent: String) -> FundingOptions? {
        state.withLock { $0.options[intent] }
    }

    func record(intent: String, options: FundingOptions) {
        state.withLock { $0.options[intent] = options }
    }

    /// Session `intent`'s overlay closed, and what it offered with it.
    func forgetOptions(intent: String) {
        state.withLock { $0.options[intent] = nil }
    }

    /// Every ended session the core recorded, newest first, once it has sent
    /// them.
    var history: [FundingSessionSummary]? {
        state.withLock { $0.history }
    }

    func record(history: [FundingSessionSummary]) {
        state.withLock { $0.history = history }
    }
}
