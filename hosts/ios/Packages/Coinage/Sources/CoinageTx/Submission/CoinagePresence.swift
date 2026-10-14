import AsyncExtensions
import Foundation

/// The coins of `publicKeys` the chain holds, on every look that could be taken.
///
/// A key that goes absent drops out of the underlying map, so an emission is the whole set visible at
/// that block. A look that cannot be taken is never emitted — the subscription simply does not yield —
/// so a failed read can never erase what the chain last showed.
func coinPresence(
    of publicKeys: Set<PublicKey>,
    reading query: any CoinOnChainQuerying
) -> AnyAsyncSequence<Set<PublicKey>> {
    query
        .subscribeCoinInfos(for: Array(publicKeys))
        .map { Set($0.keys) }
        .eraseToAnyAsyncSequence()
}

/// The vouchers our own rows currently show sitting in a recycler, on every change to them.
///
/// A voucher counts as present while it sits in a recycler: that is where an unload proves it, and one
/// that left was redeemed by something else.
///
/// Deliberately the *local* rows rather than a chain read. They are the same rows the rebuild builds
/// from, so the gate and the build can never disagree — a chain read could say "in a recycler" while
/// the row the call is built from still has no recycler, and the build would fail on every attempt
/// until sync caught up. Location sync is what keeps these rows current.
func voucherRecyclerPresence(
    snapshots: AnyAsyncSequence<[TrackedVoucher]>
) -> AnyAsyncSequence<Set<CoinageKeyIndex>> {
    snapshots
        .map { tracked in
            Set(tracked.filter { $0.voucher.recycler != nil }.map(\.voucher.derivationIndex))
        }
        .eraseToAnyAsyncSequence()
}
