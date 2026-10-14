import Foundation
import Products

/// Matches Pocket's `TLD_RETRY_INTERVAL` on Android.
private let tldRetryInterval = 3

extension DotNsTldProviding {
    /// The network TLD, waited for rather than given up on — the counterpart of Android's
    /// `getTldRetrying`, which Pocket's pinned-card list resolves its backing products through.
    ///
    /// One unpaced attempt, then polling. `resolveTld()` bypasses the provider's failure backoff,
    /// so looping on it would hammer the chain on the one device that cannot reach it;
    /// `currentTld()` re-reads through that backoff. It also answers from a persisted store, so
    /// only a first launch ever suspends here.
    ///
    /// - Parameter attempts: polls before giving up. Unbounded suits a caller whose own
    ///   cancellation ends the wait; a detached caller must bound it.
    func tldRetrying(attempts: Int = .max) async -> String? {
        if let cached = currentTld() {
            return cached
        }

        if let resolved = try? await resolveTld() {
            return resolved
        }

        for _ in 0 ..< attempts {
            do {
                try await Task.sleep(for: .seconds(tldRetryInterval))
            } catch {
                return nil
            }

            if let tld = currentTld() {
                return tld
            }
        }

        return nil
    }
}
