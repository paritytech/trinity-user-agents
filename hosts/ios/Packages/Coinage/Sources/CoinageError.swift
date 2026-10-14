import Foundation

/// Errors that can occur during CoinageService operations.
public enum CoinageError: Error, Equatable {
    /// Transfer amount exceeds available spendable balance.
    case insufficientBalance(requested: Decimal, available: Decimal)

    /// Transfer execution failed with underlying error.
    case transferFailed(underlying: Error)

    /// Persistence operation failed (save, markSpent, or delete).
    case persistenceFailed(underlying: Error)

    /// Operation called before setup(with:) was invoked.
    case notConfigured

    public static func == (lhs: CoinageError, rhs: CoinageError) -> Bool {
        switch (lhs, rhs) {
        case let (
            .insufficientBalance(lhsRequested, lhsAvailable),
            .insufficientBalance(rhsRequested, rhsAvailable)
        ):
            lhsRequested == rhsRequested && lhsAvailable == rhsAvailable
        case let (.transferFailed(lhsUnderlying), .transferFailed(underlying: rhsUnderlying)):
            String(describing: lhsUnderlying) == String(describing: rhsUnderlying)
        case let (.persistenceFailed(lhsUnderlying), .persistenceFailed(underlying: rhsUnderlying)):
            String(describing: lhsUnderlying) == String(describing: rhsUnderlying)
        case (.notConfigured, .notConfigured):
            true
        default:
            false
        }
    }
}
