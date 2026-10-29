import Foundation

/// Whether a provider still worked on a funding session when the app last
/// heard from the core. The next launch starts the core at once when it did,
/// so the providers carry on from their last checkpoint instead of waiting
/// for a screen or product to need the core.
enum FundingResumeMarker {
    fileprivate static let key = "funding.inProgress"
    private static let writer = FundingResumeMarkerWriter()

    static var inProgress: Bool {
        UserDefaults.standard.bool(forKey: key)
    }

    /// Asks the core and stores its answer. Calls are answered in the order
    /// they are made, so a later answer is never overwritten by an earlier one.
    static func record(_ runtime: FundingRuntime) {
        writer.enqueue(runtime)
    }
}

/// Chains each write after the one before it.
private final class FundingResumeMarkerWriter: @unchecked Sendable {
    private let lock = NSLock()
    private var tail: Task<Void, Never>?

    func enqueue(_ runtime: FundingRuntime) {
        lock.lock()
        defer { lock.unlock() }

        let previous = tail
        tail = Task.detached {
            await previous?.value
            guard let inProgress = try? await runtime.fundingInProgress() else { return }
            UserDefaults.standard.set(inProgress, forKey: FundingResumeMarker.key)
        }
    }
}
