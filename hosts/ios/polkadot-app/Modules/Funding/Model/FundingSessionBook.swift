import Foundation
import os
import TrUAPIHost

/// What the core last pushed about funding: each session with its progress,
/// and what each overlay offers. The core sends each with every change, and
/// again when funding resumes, so the screens read them here rather than from
/// the core.
final class FundingSessionBook: Sendable {
    static let shared = FundingSessionBook()

    struct Entry: Sendable {
        var session: FundingSession
        let progress: FundingProgress?

        /// A withdrawal stays in flight after its release until its payout
        /// is reported.
        var isInFlight: Bool {
            progress?.status.isInFlight ?? session.stage.isOpen
        }
    }

    private struct State: Sendable {
        var entries: [String: Entry] = [:]
        var options: [String: FundingOptions] = [:]
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

    /// Every session the core pushed this run, in flight and ended.
    func entries() -> [Entry] {
        state.withLock { Array($0.entries.values) }
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

    /// The user was shown how ended session `intent` went, so the core stops
    /// handing it over on each resume. Does nothing while it is in flight or
    /// once it is acknowledged.
    func acknowledgeShown(intent: String, runtime: FundingRuntime) async {
        guard setAcknowledged(intent: intent, to: true) else { return }

        do {
            _ = try await runtime.acknowledgeFundingSession(intent: intent)
        } catch {
            _ = setAcknowledged(intent: intent, to: false)
            Logger.shared.error("[funding] \(intent): acknowledging the ended session failed: \(error)")
        }
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
}

private extension FundingSessionBook {
    /// Whether this call changed it; an acknowledgement only lands on an ended
    /// session.
    func setAcknowledged(intent: String, to acknowledged: Bool) -> Bool {
        state.withLock { state in
            guard let entry = state.entries[intent], !entry.isInFlight,
                  entry.session.acknowledged != acknowledged
            else { return false }
            state.entries[intent]?.session.acknowledged = acknowledged
            return true
        }
    }
}
