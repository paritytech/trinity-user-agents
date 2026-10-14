import DurableTransactions
import Foundation
import Operation_iOS
import Testing
@testable import Coinage
@testable import polkadot_app

@Suite("Tracked asset mappers: recovered assets carry a finalized minter")
struct TrackedAssetMapperTests {
    private let facade = UserDataStorageTestFacade()

    @Test("a coin of a previous installation with no minter entry counts as minted")
    func recoveredCoinCountsAsMinted() async throws {
        try await seedCurrentInstallation()
        try await save(coin(installation: .other, key: Self.key(1)))

        let tracked = try #require(await trackedCoins().first)
        #expect(tracked.state.minterStatus == .finalizedSuccess)
        #expect(tracked.state.handedOff == false)
    }

    @Test("a coin of this installation with no minter entry has no minter status")
    func ownOrphanStaysUnminted() async throws {
        try await seedCurrentInstallation()
        try await save(coin(installation: .test, key: Self.key(2)))

        let tracked = try #require(await trackedCoins().first)
        #expect(tracked.state.minterStatus == nil)
    }

    @Test("a recorded minter of a previous installation is reported as it stands")
    func recordedMinterWins() async throws {
        try await seedCurrentInstallation()
        let minted = coin(installation: .other, key: Self.key(3))
        try await save(minted)
        try await CoinageCoreDataLedger(storageFacade: facade).register([
            CoinageTxRegistration(
                txHash: Data(repeating: 3, count: 32),
                checkpoint: BlockRef(number: 100, hash: Data([100])),
                mortalityBlocks: 64,
                groupId: nil,
                inputs: [],
                outputs: [.coin(minted.derivationIndex, minted.publicKey)]
            )
        ])

        let tracked = try #require(await trackedCoins().first)
        #expect(tracked.state.minterStatus == .pending)
    }

    @Test("without a current installation row nothing counts as recovered")
    func noCurrentRow() async throws {
        try await save(coin(installation: .other, key: Self.key(4)))

        let tracked = try #require(await trackedCoins().first)
        #expect(tracked.state.minterStatus == nil)
    }

    @Test("a voucher of a previous installation with no minter entry counts as minted")
    func recoveredVoucherCountsAsMinted() async throws {
        try await seedCurrentInstallation()
        try await save(voucher(installation: .other, key: Self.key(5)))

        let tracked = try #require(await trackedVouchers().first)
        #expect(tracked.state.minterStatus == .finalizedSuccess)
    }

    @Test("a voucher of this installation with no minter entry has no minter status")
    func ownVoucherStaysUnminted() async throws {
        try await seedCurrentInstallation()
        try await save(voucher(installation: .test, key: Self.key(6)))

        let tracked = try #require(await trackedVouchers().first)
        #expect(tracked.state.minterStatus == nil)
    }

    /// The reported hang: yields `.detecting` without the derived minter.
    @Test("a handed-off recovered coin that vanished at finality is a finalized claim")
    func handedOffRecoveredCoinIsClaimed() async throws {
        try await seedCurrentInstallation()
        let key = Self.key(7)
        try await save(coin(installation: .other, key: key, isOnchain: false, handoffMark: .committed))

        let tracked = try #require(await trackedCoins().first)
        let status = CoinageTransferStatusService.transferStatus(tracked, atFinalized: [key: false])
        #expect(status == .claimed(finalized: true))
    }
}

private extension TrackedAssetMapperTests {
    static func key(_ byte: UInt8) -> PublicKey {
        Data(repeating: byte, count: 32)
    }

    func seedCurrentInstallation() async throws {
        _ = try await CoinageCurrentInstallationCoreDataRepository(storageFacade: facade)
            .getOrCreateCurrent { .test }
    }

    func coin(
        installation: CoinageInstallationId,
        key: PublicKey,
        isOnchain: Bool = true,
        handoffMark: CoinHandoffMark = .none
    ) -> Coin {
        Coin(
            exponent: 1,
            derivationIndex: CoinageKeyIndex(installation: installation, item: 4),
            age: 3,
            isOnchain: isOnchain,
            handoffMark: handoffMark,
            publicKey: key
        )
    }

    func voucher(installation: CoinageInstallationId, key: PublicKey) -> Voucher {
        Voucher(
            exponent: 1,
            derivationIndex: CoinageKeyIndex(installation: installation, item: 2),
            allocatedAt: .now,
            readyAt: .distantPast,
            remoteState: .onboarding,
            publicKey: key
        )
    }

    func save(_ coin: Coin) async throws {
        try await facade.makeRepo(mapper: CoinMapper()).saveOperation({ [coin] }, { [] }).asyncExecute()
    }

    func save(_ voucher: Voucher) async throws {
        try await facade.makeRepo(mapper: VoucherMapper()).saveOperation({ [voucher] }, { [] }).asyncExecute()
    }

    func trackedCoins() async throws -> [TrackedCoin] {
        try await facade.makeRepo(mapper: TrackedCoinMapper())
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
    }

    func trackedVouchers() async throws -> [TrackedVoucher] {
        try await facade.makeRepo(mapper: TrackedVoucherMapper())
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
    }
}
