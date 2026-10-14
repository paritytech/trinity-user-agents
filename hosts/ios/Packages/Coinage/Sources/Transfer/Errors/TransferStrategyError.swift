import Foundation

/// Errors that can occur during transfer strategy execution.
public enum TransferStrategyError: Error {
    /// No coins provided for exact match strategy
    case emptyCoins
    /// No vouchers provided for unload strategy
    case emptyVouchers
    /// Voucher missing recycler information required for unload
    case missingRecyclerInfo
    /// Extrinsic submission failed
    case submissionFailed(Error)
    /// Index allocation failed
    case allocationFailed(Error)
    /// Failed on fetching correct recycler revision
    case invalidRecyclerRevision
    /// Multiple tasks failed; all errors are collected here
    case multiple([Error])
}

extension TransferStrategyError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .emptyCoins:
            "No coins to transfer"
        case .emptyVouchers:
            "No vouchers to unload"
        case .missingRecyclerInfo:
            "Voucher is missing recycler info"
        case .invalidRecyclerRevision:
            "Unexpected recycler revision"
        case let .submissionFailed(error):
            "Submission failed: \(Self.describe(error))"
        case let .allocationFailed(error):
            "Index allocation failed: \(Self.describe(error))"
        case let .multiple(errors):
            "\(errors.count) tasks failed: \(errors.map(Self.describe).joined(separator: "; "))"
        }
    }
}

private extension TransferStrategyError {
    /// Mirrors the staleness reporter's own unwrapping: a pure-Swift error's
    /// `localizedDescription` is the useless NSError bridge string, so prefer an explicit
    /// `LocalizedError` description.
    static func describe(_ error: Error) -> String {
        if let localized = error as? LocalizedError, let description = localized.errorDescription {
            return description
        }

        return String(describing: error)
    }
}
