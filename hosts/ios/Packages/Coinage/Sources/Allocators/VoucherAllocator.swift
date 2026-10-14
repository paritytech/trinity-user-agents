import Foundation
import SubstrateSdk
import NovaCrypto
import Operation_iOS

protocol VoucherAllocating: Actor {
    func allocate(exponent: Int16) async throws -> Voucher
}

/// Hands out the next voucher item in the current installation from the Keychain-backed counter, so a
/// previous installation's vouchers never move this installation's counter. The store reserves each
/// item atomically, so concurrent mints never collide.
actor VoucherAllocator: VoucherAllocating {
    private let installationStore: any CoinageCurrentInstallationStoring
    private let delayProvider: VoucherDelayProviderProtocol
    private let voucherRepository: AnyDataProviderRepository<Voucher>
    private let keyFactory: any VoucherKeyDeriving

    init(
        installationStore: any CoinageCurrentInstallationStoring,
        delayProvider: VoucherDelayProviderProtocol,
        voucherRepository: AnyDataProviderRepository<Voucher>,
        keyFactory: any VoucherKeyDeriving
    ) {
        self.installationStore = installationStore
        self.delayProvider = delayProvider
        self.voucherRepository = voucherRepository
        self.keyFactory = keyFactory
    }

    /// Allocates a new voucher index and persists the voucher — with its on-chain public key cached
    /// so the durability layer never re-derives it — from the moment it is minted.
    func allocate(exponent: Int16) async throws -> Voucher {
        let index = try await nextIndex()
        let delay = delayProvider.timeInterval()
        let allocatedAt = Date.now

        let voucher = try Voucher(
            exponent: exponent,
            derivationIndex: index,
            allocatedAt: allocatedAt,
            readyAt: allocatedAt.addingTimeInterval(delay),
            publicKey: keyFactory.derivePublicKey(index: index)
        )
        try await voucherRepository.saveOperation({ [voucher] }, { [] }).asyncExecute()
        return voucher
    }
}

private extension VoucherAllocator {
    func nextIndex() async throws -> CoinageKeyIndex {
        let installation = try await installationStore.getOrCreateCurrent()
        return try await CoinageKeyIndex(installation: installation, item: installationStore.nextVoucherItem())
    }
}

protocol VoucherDelayProviderProtocol {
    func timeInterval() -> TimeInterval
}

final class VoucherDelayProvider: VoucherDelayProviderProtocol {
    private let maxWaitTime: TimeInterval = CoinageConstants.maxVoucherWaitTime

    func timeInterval() -> TimeInterval {
        TimeInterval.random(in: 0 ... maxWaitTime)
    }
}
