import AsyncExtensions
import DurableTransactions
import Foundation
import Testing
@testable import Coinage

/// An unload rebuilt from its ledger row redeems exactly the vouchers it registered and mints exactly
/// the coins it recorded. A voucher counts as present only while it sits in a recycler — that is where
/// an unload proves it, and one that has left was redeemed by something else.
@Suite("Unload Rebuild")
struct UnloadRebuildTests {
    private let policyId = CoinageSubmissionParams.unloadPolicyId

    @Test("terms come from the transfer params")
    func termsAreDecoded() throws {
        let rebuild = makeRebuild(coins: [], vouchers: [])

        let terms = try #require(rebuild.terms(of: RebuildFixtures.transferParams(inSeconds: 60)))

        #expect(terms.deadline == RebuildFixtures.now.addingTimeInterval(60))
        #expect(terms.retriesFailures)
    }

    @Test("params that cannot be read have no terms")
    func unreadableParamsHaveNoTerms() {
        let rebuild = makeRebuild(coins: [], vouchers: [])

        #expect(rebuild.terms(of: Data([0xFF])) == nil)
    }

    @Test("the recorded vouchers and coins are resolved into an unload")
    func resolvesRecordedUnload() async throws {
        let vouchers = [RebuildFixtures.voucher(1), RebuildFixtures.voucher(2)]
        let outputs = [RebuildFixtures.coin(5)]
        let rebuild = makeRebuild(coins: outputs, vouchers: vouchers)
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: vouchers.map { .recyclerVoucher($0.derivationIndex, $0.publicKey) },
            outputs: outputs.map { .coin($0.derivationIndex, $0.publicKey) }
        )]

        let unload = try #require(try await rebuild.resolve([transaction], assets: assets)[transaction.id])

        #expect(unload.voucherIndices == vouchers.map(\.derivationIndex))
        #expect(unload.outputs.map(\.publicKey) == outputs.map(\.publicKey))
    }

    @Test("every recorded voucher is waited on")
    func inputsAreTheVouchers() async throws {
        let vouchers = [RebuildFixtures.voucher(1), RebuildFixtures.voucher(2)]
        let outputs = [RebuildFixtures.coin(5)]
        let rebuild = makeRebuild(coins: outputs, vouchers: vouchers)
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: vouchers.map { .recyclerVoucher($0.derivationIndex, $0.publicKey) },
            outputs: outputs.map { .coin($0.derivationIndex, $0.publicKey) }
        )]

        let unload = try #require(try await rebuild.resolve([transaction], assets: assets)[transaction.id])

        #expect(rebuild.inputs(of: unload) == [1, 2])
    }

    @Test("a voucher the wallet no longer holds resolves to nothing")
    func missingVoucherResolvesToNothing() async throws {
        let held = RebuildFixtures.voucher(1)
        let outputs = [RebuildFixtures.coin(5)]
        let rebuild = makeRebuild(coins: outputs, vouchers: [held])
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [
                .recyclerVoucher(1, held.publicKey),
                .recyclerVoucher(2, RebuildFixtures.key(2))
            ],
            outputs: outputs.map { .coin($0.derivationIndex, $0.publicKey) }
        )]

        let resolved = try await rebuild.resolve([transaction], assets: assets)

        #expect(resolved[transaction.id] == nil)
    }

    @Test("a recorded output coin the wallet lost resolves to nothing")
    func missingOutputResolvesToNothing() async throws {
        let vouchers = [RebuildFixtures.voucher(1)]
        let rebuild = makeRebuild(coins: [], vouchers: vouchers)
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.recyclerVoucher(1, vouchers[0].publicKey)],
            outputs: [.coin(5, RebuildFixtures.key(5))]
        )]

        let resolved = try await rebuild.resolve([transaction], assets: assets)

        #expect(resolved[transaction.id] == nil)
    }

    @Test("an input that is not a recycler voucher resolves to nothing")
    func coinInputResolvesToNothing() async throws {
        let coin = RebuildFixtures.coin(1)
        let outputs = [RebuildFixtures.coin(5)]
        let rebuild = makeRebuild(coins: [coin] + outputs, vouchers: [])
        let transaction = try scheduled()
        let assets = [transaction.id: RebuildFixtures.entry(
            id: transaction.id,
            inputs: [.coin(.own(1, coin.publicKey))],
            outputs: outputs.map { .coin($0.derivationIndex, $0.publicKey) }
        )]

        let resolved = try await rebuild.resolve([transaction], assets: assets)

        #expect(resolved[transaction.id] == nil)
    }

    @Test("a row with no inputs or no outputs resolves to nothing")
    func emptySidesResolveToNothing() async throws {
        let voucher = RebuildFixtures.voucher(1)
        let coin = RebuildFixtures.coin(5)
        let rebuild = makeRebuild(coins: [coin], vouchers: [voucher])
        let noOutputs = try scheduled()
        let noInputs = try scheduled()
        let assets = [
            noOutputs.id: RebuildFixtures.entry(
                id: noOutputs.id,
                inputs: [.recyclerVoucher(1, voucher.publicKey)],
                outputs: []
            ),
            noInputs.id: RebuildFixtures.entry(
                id: noInputs.id,
                inputs: [],
                outputs: [.coin(5, coin.publicKey)]
            )
        ]

        let resolved = try await rebuild.resolve([noOutputs, noInputs], assets: assets)

        #expect(resolved.isEmpty)
    }

    // MARK: - The recycler gate

    @Test("only vouchers sitting in a recycler count as present")
    func onlyRecyclerVouchersArePresent() async throws {
        let inRecycler = RebuildFixtures.voucher(1)
        let elsewhere = RebuildFixtures.voucher(2, inRecycler: false)
        let rebuild = makeRebuild(coins: [], vouchers: [inRecycler, elsewhere])

        let looks = try await rebuild.presence(of: [1, 2])
        var seen: Set<CoinageKeyIndex>?
        for try await look in looks {
            seen = look
            break
        }

        #expect(seen == [1])
    }
}

private extension UnloadRebuildTests {
    func makeRebuild(coins: [Coin], vouchers: [Voucher]) -> UnloadRebuild {
        let voucherService = StubVoucherService(vouchers: vouchers)

        return UnloadRebuild(
            coinService: StubCoinService(coins: RebuildFixtures.tracked(coins)),
            voucherService: voucherService,
            builder: UnloadExtrinsicBuilder(
                instanceId: 0,
                voucherKeyFactory: StubVoucherKeyFactory(),
                recyclerLoader: StubRecyclerReadinessLoader(),
                originFactory: StubOriginFactory(),
                blockInfoProvider: TransferSenderServiceTests.MockBlockNumberProvider(),
                quotaTracker: StubUnloadQuotaTracker(),
                factory: StubTxFactory(),
                chainId: "test-chain",
                logger: nil
            ),
            voucherSnapshots: {
                AsyncStream<[TrackedVoucher]> { continuation in
                    continuation.yield(vouchers.map { TrackedVoucher(voucher: $0, state: RebuildFixtures.freeState) })
                    continuation.finish()
                }
                .eraseToAnyAsyncSequence()
            },
            dateProvider: StubDateProvider(RebuildFixtures.now)
        )
    }

    func scheduled() throws -> ScheduledDurableTx {
        try RebuildFixtures.scheduled(
            policyId: policyId,
            params: RebuildFixtures.transferParams(inSeconds: 60)
        )
    }
}
