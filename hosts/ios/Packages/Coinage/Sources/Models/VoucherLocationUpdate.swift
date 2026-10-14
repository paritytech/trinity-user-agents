import Operation_iOS

/// What a location sync resolves for a voucher — its `remoteState` (onboarding / in-recycler) and,
/// once it is in a ring, that ring's fungibility — keyed by derivation index. Persisted through a
/// dedicated write-only mapper so a location write only ever touches these fields, never
/// reading-then-overwriting the rest of the voucher.
public struct VoucherLocationUpdate: Equatable, Sendable {
    public let derivationIndex: CoinageKeyIndex
    public let remoteState: Voucher.OnChainState

    /// The ring's fungibility as of this reading. `nil` leaves the stored value alone — the voucher
    /// has no ring yet, or its capacity is not resolved, so there is nothing to say.
    public let recyclerFungibility: UInt8?

    /// The ceiling the ring could still reach, written only once. `nil` leaves the stored value
    /// alone; the mapper additionally refuses to overwrite a ceiling that is already set.
    public let maxRecyclerFungibility: UInt8?

    public init(
        derivationIndex: CoinageKeyIndex,
        remoteState: Voucher.OnChainState,
        recyclerFungibility: UInt8? = nil,
        maxRecyclerFungibility: UInt8? = nil
    ) {
        self.derivationIndex = derivationIndex
        self.remoteState = remoteState
        self.recyclerFungibility = recyclerFungibility
        self.maxRecyclerFungibility = maxRecyclerFungibility
    }
}

extension VoucherLocationUpdate: Operation_iOS.Identifiable {
    public var identifier: String { Voucher.identifier(for: derivationIndex) }
}
