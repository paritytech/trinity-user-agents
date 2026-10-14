import AsyncExtensions
import Operation_iOS

/// Protocol for factories that provide database repositories for Coinage.
/// Implemented in the main app target to bridge CoreData infrastructure to the package.
public protocol DatabaseDependencyFactoring: Sendable {
    func makeCoinRepository() -> AnyDataProviderRepository<Coin>
    /// A coin repository scoped to the given public keys — a filtered query rather than the whole set.
    func makeCoinRepository(publicKeys: [PublicKey]) -> AnyDataProviderRepository<Coin>
    func makeTrackedCoinRepository() -> AnyDataProviderRepository<TrackedCoin>
    /// A write-only repository for chain-sync presence updates (`age`, `isOnchain`), backed by a
    /// mapper that touches only those fields — see ``CoinPresenceUpdate``.
    func makeCoinPresenceRepository() -> AnyDataProviderRepository<CoinPresenceUpdate>
    func makeVoucherRepository() -> AnyDataProviderRepository<Voucher>
    /// A voucher repository scoped to the given public keys — a filtered query rather than the whole
    /// set. Empty `publicKeys` still returns a repository (the caller guards the empty case).
    func makeVoucherRepository(publicKeys: [PublicKey]) -> AnyDataProviderRepository<Voucher>
    func makeTrackedVoucherRepository() -> AnyDataProviderRepository<TrackedVoucher>
    func makeTrackedVoucherRepository(derivationIndices: [CoinageKeyIndex]) -> AnyDataProviderRepository<TrackedVoucher>
    /// A write-only repository for location-sync updates (`remoteState`, `privacy`), backed by a
    /// mapper that touches only those fields — see ``VoucherLocationUpdate``.
    func makeVoucherLocationRepository() -> AnyDataProviderRepository<VoucherLocationUpdate>

    /// A stream of full ``TrackedCoin`` snapshots — the current set, then a fresh snapshot on every
    /// CoreData save. Re-emits when an entry's status change touches a coin's relations, so the
    /// durability overlay stays current without a separate change-merge.
    func makeTrackedCoinSnapshotStream() -> AnyAsyncSequence<[TrackedCoin]>
    /// The same, restricted to coins with the given public keys — a filtered subscription rather than
    /// the whole set, for callers that only track a handful of coins (e.g. a transfer's handed-off
    /// coins). Empty `publicKeys` yields an empty stream.
    func makeTrackedCoinSnapshotStream(publicKeys: [PublicKey]) -> AnyAsyncSequence<[TrackedCoin]>
    /// The voucher analogue of ``makeTrackedCoinSnapshotStream()``.
    func makeTrackedVoucherSnapshotStream() -> AnyAsyncSequence<[TrackedVoucher]>

    /// The installations this seed owns — see ``CoinageInstallationRepositoryProtocol``.
    func makeInstallationRepository() -> any CoinageInstallationRepositoryProtocol
}
