import Foundation
import BandersnatchApi
import SubstrateSdk
import ExtrinsicService
import StructuredConcurrency
import BigInt
import KeyDerivation
import Keystore_iOS
import SDKLogger
import Operation_iOS

public protocol VoucherServiceProtocol: Sendable {
    /// Loads vouchers for `amount` and returns them. `groupId` is forwarded to the durability layer
    /// so a caller can watch the load settle as one operation; `nil` leaves the entries ungrouped.
    @discardableResult
    func load(
        amount: BigUInt,
        externalAssetHolder: any WalletManaging,
        breakdownContext: DenominationBreakdownContext,
        groupId: CoinageTxGroupId?
    ) async throws -> [Voucher]

    /// Fetch all vouchers paired with their derived durability overlay.
    func fetchAllTracked() async throws -> [TrackedVoucher]

    /// Fetch only the vouchers with the given public keys — a filtered query, not the whole set.
    /// Empty `publicKeys` returns an empty array without touching the store.
    func fetchVouchers(publicKeys: Set<PublicKey>) async throws -> [Voucher]

    /// Fetch only the tracked vouchers with the given derivation indices — a filtered query, not the
    /// whole set. Empty `derivationIndices` returns an empty array without touching the store.
    func fetchTracked(derivationIndices: Set<CoinageKeyIndex>) async throws -> [TrackedVoucher]
}

public final class VoucherService: @unchecked Sendable {
    private let databaseFactory: any DatabaseDependencyFactoring
    private let trackedVoucherRepository: AnyDataProviderRepository<TrackedVoucher>
    private let voucherLoaderFactory: VoucherLoaderFactoryProtocol

    public init(
        databaseFactory: any DatabaseDependencyFactoring,
        trackedVoucherRepository: AnyDataProviderRepository<TrackedVoucher>,
        voucherLoaderFactory: VoucherLoaderFactoryProtocol
    ) {
        self.databaseFactory = databaseFactory
        self.trackedVoucherRepository = trackedVoucherRepository
        self.voucherLoaderFactory = voucherLoaderFactory
    }
}

extension VoucherService: VoucherServiceProtocol {
    @discardableResult
    public func load(
        amount: BigUInt,
        externalAssetHolder: any WalletManaging,
        breakdownContext: DenominationBreakdownContext,
        groupId: CoinageTxGroupId?
    ) async throws -> [Voucher] {
        let loader = try voucherLoaderFactory.makeLoader(for: externalAssetHolder)
        // Vouchers are persisted by the allocator as they are minted.
        return try await loader.load(amount: amount, breakdownContext: breakdownContext, groupId: groupId)
    }

    public func fetchAllTracked() async throws -> [TrackedVoucher] {
        try await trackedVoucherRepository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute()
    }

    public func fetchVouchers(publicKeys: Set<PublicKey>) async throws -> [Voucher] {
        guard !publicKeys.isEmpty else { return [] }
        return try await databaseFactory.makeVoucherRepository(publicKeys: Array(publicKeys))
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
    }

    public func fetchTracked(derivationIndices: Set<CoinageKeyIndex>) async throws -> [TrackedVoucher] {
        guard !derivationIndices.isEmpty else { return [] }
        return try await databaseFactory.makeTrackedVoucherRepository(derivationIndices: Array(derivationIndices))
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
    }
}
