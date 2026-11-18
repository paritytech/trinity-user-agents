import Foundation

/// Where recovery of the wallet's balance stands. The initial phase is the scan of the wallet the app
/// handed over to the core; a deep search is the user asking for another look; `completed` is the user
/// having accepted the balance. A phase that `failed` found nothing it can vouch for, so no balance is
/// offered for acceptance.
public enum BackupProgress: Hashable, Sendable {
    public enum Phase: Hashable, Sendable {
        case syncing
        case completed
        case failed
    }

    case unknown
    case initial(Phase)
    case deep(Phase)
    case completed

    public var isInProgress: Bool {
        switch self {
        case .initial(.syncing),
             .deep(.syncing): true
        case .unknown,
             .initial(.completed),
             .initial(.failed),
             .deep(.completed),
             .deep(.failed),
             .completed: false
        }
    }

    /// A recovered balance the user has not accepted yet.
    public var awaitsAcknowledgement: Bool {
        switch self {
        case .initial(.completed),
             .deep(.completed): true
        case .unknown,
             .initial(.syncing),
             .initial(.failed),
             .deep(.syncing),
             .deep(.failed),
             .completed: false
        }
    }
}
