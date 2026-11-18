import Foundation

/// Whether this installation can be recovered from the seed, which takes its registration in the
/// installation registry.
public enum CoinageAccountBackupStatus: Hashable, Sendable {
    case registering
    /// Not registered, and not about to be: no registry is configured.
    case delayed
    case completed
}
