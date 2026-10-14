import Foundation

/// A planned memo entry describing a coin that will be included in the transfer memo.
/// Created during transfer planning, before coin allocation and key derivation.
struct PlannedMemoEntry: Equatable {
    /// The derivation index of the coin (used to derive the private key for the memo).
    let coinDerivationIndex: CoinageKeyIndex

    /// The denomination exponent (power-of-two value).
    let valueExponent: Int16
}
