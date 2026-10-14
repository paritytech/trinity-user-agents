import DurableTransactionsTestSupport
import Foundation
import Individuality
import SubstrateSdk
import Testing
@testable import Coinage

@Suite("Installation asset scanner reads the finalized head")
struct InstallationAssetScannerTests {
    private let chain = CoinageFakeChain(initialState: .empty)
    private let chainFactory: FakePinnedChainViewFactory<CoinageChainState>
    private let coinQuery = StubCoinOnChainQuery()
    private let voucherQuery = StubVoucherOnChainQuery()
    private let keyFactory = CoinKeypairFactory(entropyManager: MockEntropyManager(entropy: Self.entropy))
    private let scanner: InstallationAssetScanner

    init() {
        // Finalized and best heads differ, so a read at the wrong one is visible.
        chain.produceBlock()
        chain.produceBlock()
        chain.finalize(upTo: 1)
        chainFactory = FakePinnedChainViewFactory(chain: chain)
        scanner = InstallationAssetScanner(
            coinKeypairFactory: keyFactory,
            coinOnChainQuery: coinQuery,
            voucherOnChainQuery: voucherQuery,
            chainViewFactory: chainFactory,
            chainId: Self.chainId,
            logger: nil
        )
    }

    @Test("coins are read at the finalized head, not the best one")
    func coinsAreReadAtFinalizedHead() async throws {
        _ = try await scanner.scanCoins(installation: .other, startIndex: 0, count: 2)

        #expect(coinQuery.reads.map(\.atBlockHash) == [chain.finalizedHead.hash])
        #expect(chain.finalizedHead.hash != chain.bestHead.hash)
    }

    @Test("a coin the finalized head holds is recovered at its index, on chain, with its age")
    func recoveredCoinKeepsIndexAndAge() async throws {
        let index = CoinageKeyIndex(installation: .other, item: 1)
        let key = try keyFactory.derivePublicKey(index: index)
        coinQuery.setPresent(key, atBlockHash: chain.finalizedHead.hash, value: 3, age: 7)
        // Present on the best head only: must not be recovered.
        let unfinalizedKey = try keyFactory.derivePublicKey(index: CoinageKeyIndex(installation: .other, item: 0))
        coinQuery.setPresent(unfinalizedKey, atBlockHash: chain.bestHead.hash)

        let coins = try await scanner.scanCoins(installation: .other, startIndex: 0, count: 2)

        let coin = try #require(coins.first)
        #expect(coins.count == 1)
        #expect(coin.derivationIndex == index)
        #expect(coin.exponent == 3)
        #expect(coin.age == 7)
        #expect(coin.isOnchain)
        #expect(coin.publicKey == key)
    }

    @Test("vouchers are read at the finalized head, not the best one")
    func vouchersAreReadAtFinalizedHead() async throws {
        let index = CoinageKeyIndex(installation: .other, item: 1)
        voucherQuery.setInfo(Self.onboardingVoucher(index), for: index, atBlockHash: chain.finalizedHead.hash)

        let vouchers = try await scanner.scanVouchers(installation: .other, startIndex: 0, count: 2)

        #expect(voucherQuery.reads.map(\.atBlockHash) == [chain.finalizedHead.hash])
        #expect(vouchers.map(\.derivationIndex) == [index])
        #expect(vouchers.first?.remoteState == .onboarding)
    }

    @Test("a scan that cannot pin a view throws instead of reading the best head")
    func pinFailureThrows() async throws {
        chainFactory.faults.pinFails = true

        await #expect(throws: (any Error).self) {
            try await scanner.scanCoins(installation: .other, startIndex: 0, count: 2)
        }
        await #expect(throws: (any Error).self) {
            try await scanner.scanVouchers(installation: .other, startIndex: 0, count: 2)
        }
        #expect(coinQuery.reads.isEmpty)
        #expect(voucherQuery.reads.isEmpty)
    }
}

private extension InstallationAssetScannerTests {
    static let entropy = Data(repeating: 0xAB, count: 32)
    static let chainId: ChainId = "scanner-chain"

    static func onboardingVoucher(_ index: CoinageKeyIndex) -> VoucherOnChainInfo {
        VoucherOnChainInfo(
            publicKey: Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32),
            exponent: 2,
            ringPosition: .onboarding(.init(queuePage: 0, queuedAt: 0)),
            ringMembersCount: nil,
            aliasPresence: .unknown
        )
    }
}
