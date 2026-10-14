import Foundation

/// Where recovery of previous installations' balance stands. The initial pass runs on launch; a deep
/// search is the user asking for another look; `completed` is the user having accepted the balance.
/// A phase that `failed` scanned nothing it can vouch for, so no balance is offered for acceptance;
/// the next launch scans again.
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

    public var isFailed: Bool {
        switch self {
        case .initial(.failed),
             .deep(.failed): true
        case .unknown,
             .initial(.syncing),
             .initial(.completed),
             .deep(.syncing),
             .deep(.completed),
             .completed: false
        }
    }
}
