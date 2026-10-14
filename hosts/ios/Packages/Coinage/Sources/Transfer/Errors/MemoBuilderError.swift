import Foundation

/// Errors that can occur during memo building.
enum MemoBuilderError: Error {
    /// No coins provided for memo
    case emptyCoins
    /// Private key derivation failed
    case keyDerivationFailed(Error)
}
