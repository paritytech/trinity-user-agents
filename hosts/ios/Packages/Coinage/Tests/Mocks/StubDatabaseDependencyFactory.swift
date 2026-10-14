import AsyncExtensions
import Foundation
import Operation_iOS
@testable import Coinage

/// Serves only the tracked-coin snapshot streams; anything else is a loud failure.
final class StubDatabaseDependencyFactory: DatabaseDependencyFactoring, @unchecked Sendable {
    let trackedCoins = AsyncCurrentValueSubject<[TrackedCoin]?>(nil)

    func makeTrackedCoinSnapshotStream(publicKeys: [PublicKey]) -> AnyAsyncSequence<[TrackedCoin]> {
        guard !publicKeys.isEmpty else {
            return AsyncStream<[TrackedCoin]> { $0.finish() }.eraseToAnyAsyncSequence()
        }
        let requested = Set(publicKeys)
        return trackedCoins
            .compactMap { $0 }
            .map { $0.filter { requested.contains($0.coin.publicKey) } }
            .eraseToAnyAsyncSequence()
    }

    func makeTrackedCoinSnapshotStream() -> AnyAsyncSequence<[TrackedCoin]> {
        trackedCoins.compactMap { $0 }.eraseToAnyAsyncSequence()
    }

    func makeCoinRepository() -> AnyDataProviderRepository<Coin> { unsupported() }
    func makeCoinRepository(publicKeys _: [PublicKey]) -> AnyDataProviderRepository<Coin> { unsupported() }
    func makeTrackedCoinRepository() -> AnyDataProviderRepository<TrackedCoin> { unsupported() }
    func makeCoinPresenceRepository() -> AnyDataProviderRepository<CoinPresenceUpdate> { unsupported() }
    func makeVoucherRepository() -> AnyDataProviderRepository<Voucher> { unsupported() }
    func makeVoucherRepository(publicKeys _: [PublicKey]) -> AnyDataProviderRepository<Voucher> { unsupported() }
    func makeTrackedVoucherRepository() -> AnyDataProviderRepository<TrackedVoucher> { unsupported() }

    func makeTrackedVoucherRepository(
        derivationIndices _: [CoinageKeyIndex]
    ) -> AnyDataProviderRepository<TrackedVoucher> { unsupported() }

    func makeVoucherLocationRepository() -> AnyDataProviderRepository<VoucherLocationUpdate> { unsupported() }
    func makeTrackedVoucherSnapshotStream() -> AnyAsyncSequence<[TrackedVoucher]> { unsupported() }
    func makeInstallationRepository() -> any CoinageInstallationRepositoryProtocol { unsupported() }

    private func unsupported(_ function: String = #function) -> Never {
        fatalError("StubDatabaseDependencyFactory does not provide \(function)")
    }
}
