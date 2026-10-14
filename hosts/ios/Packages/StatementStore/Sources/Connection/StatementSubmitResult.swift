import Foundation

public enum StatementSubmitResult: Equatable {
    struct RawModel: Decodable {
        let status: String
        let reason: String?
    }

    /// Statement was accepted as new.
    case new

    /// Statement was already known.
    case known

    /// Statement was already known but has expired.
    case knownExpired

    /// Statement was rejected because the store is full or priority is too low.
    case rejected(StatementRejectionReason?)

    /// Statement failed validation.
    case invalid(StatementInvalidReason?)

    /// Internal store error.
    case internalError(String?)

    case unexpectedStatus(String, String?)

    init(rawModel: RawModel) {
        switch rawModel.status {
        case "new":
            self = .new
        case "known":
            self = .known
        case "knownExpired":
            self = .knownExpired
        case "rejected":
            if let rawReason = rawModel.reason, let reason = StatementRejectionReason(rawValue: rawReason) {
                self = .rejected(reason)
            } else {
                self = .rejected(nil)
            }
        case "invalid":
            if let rawReason = rawModel.reason, let reason = StatementInvalidReason(rawValue: rawReason) {
                self = .invalid(reason)
            } else {
                self = .invalid(nil)
            }
        case "internalError":
            self = .internalError(rawModel.reason)
        default:
            self = .unexpectedStatus(rawModel.status, rawModel.reason)
        }
    }

    func ensureSuccess() throws {
        switch self {
        case .new,
             .known,
             .knownExpired:
            return
        case let .rejected(reason):
            throw StatementSubmitError.rejected(reason)
        case let .invalid(reason):
            throw StatementSubmitError.invalid(reason)
        case let .internalError(reason):
            throw StatementSubmitError.internalError(reason)
        case let .unexpectedStatus(status, reason):
            throw StatementSubmitError.unexpectedStatus(status, reason)
        }
    }
}

public enum StatementRejectionReason: String, Equatable {
    /// Statement data exceeds the maximum allowed size for the account.
    case dataTooLarge

    /// Attempting to replace a channel message with lower or equal expiry.
    case channelPriorityTooLow

    /// Account reached its statement limit and submitted expiry is too low to evict existing.
    case accountFull

    /// The global statement store is full and cannot accept new statements.
    case storeFull

    /// Account has no allowance set.
    case noAllowance
}

public enum StatementInvalidReason: String, Equatable {
    /// Statement has no proof.
    case noProof

    /// Proof validation failed.
    case badProof

    /// Statement exceeds max allowed statement size.
    case encodingTooLarge

    /// Statement has already expired. The expiry field is in the past.
    case alreadyExpired
}

public enum StatementSubmitError: Error, Equatable {
    case rejected(StatementRejectionReason?)
    case invalid(StatementInvalidReason?)
    case internalError(String?)
    case unexpectedStatus(String, String?)
}
