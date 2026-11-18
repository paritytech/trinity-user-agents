import Foundation
import SubstrateSdk

/// Whether a holding can be spent now; everything else is still gaining privacy or arriving.
public enum CoinageAvailability: Equatable, Sendable {
    /// Free to use now at no privacy cost.
    case availableNow
    /// Held back to earn privacy, or still arriving.
    case notReady
}

/// One coin or recycler entry of the wallet, as the wallet card draws it.
public struct CoinageHolding: Equatable, Sendable {
    public enum Kind: String, Equatable, Sendable {
        case coin
        case voucher
    }

    public let kind: Kind
    /// The installation the asset was allocated under, which together with ``index`` identifies it.
    public let installation: Data
    public let index: UInt32
    /// Value is `unit · 2^exponent`.
    public let exponent: Int16
    public let availability: CoinageAvailability
    /// Payments this holding has been through. A batch-unloaded coin starts at age one, so it has none.
    public let hops: Int
    /// A coin the chain marked as one of a batch unload.
    public let isBatchUnloaded: Bool
    /// How much of its ring's anonymity the holding hides in, in percent; `nil` when unknown.
    public let fungibility: UInt8?

    public init(
        kind: Kind,
        installation: Data,
        index: UInt32,
        exponent: Int16,
        availability: CoinageAvailability,
        hops: Int,
        isBatchUnloaded: Bool,
        fungibility: UInt8?
    ) {
        self.kind = kind
        self.installation = installation
        self.index = index
        self.exponent = exponent
        self.availability = availability
        self.hops = hops
        self.isBatchUnloaded = isBatchUnloaded
        self.fungibility = fungibility
    }

    /// Stable across changes, so a row keeps its place and identity on screen.
    public var id: String {
        "\(kind.rawValue)-\(installation.toHex())-\(index)"
    }
}

/// Every holding the user still owns. Emitted from the same snapshot as ``CoinageBalance``, so a
/// display built from these rows and the balance shown beside it cannot disagree.
public struct CoinageHoldings: Equatable, Sendable {
    public let rows: [CoinageHolding]

    public init(rows: [CoinageHolding]) {
        self.rows = rows
    }

    public static let empty = CoinageHoldings(rows: [])
}
