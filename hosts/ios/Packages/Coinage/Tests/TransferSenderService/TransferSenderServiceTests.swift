import DurableTransactionsTestSupport
import Testing
import Foundation
import BigInt
import SubstrateSdk
import Individuality
import Operation_iOS
import ExtrinsicService
import KeyDerivation
import SubstrateOperation
import BackgroundExecution

@testable import Coinage

/// End-to-end tests for TransferSenderService using real strategy classes.
///
/// Tests the complete flow: TransferSenderService -> CoinSelector -> TransferPlanFactory -> Strategy.
/// The strategies drive persistence and registration through the minter and the durability store,
/// so the observable surface is `mockMinter.mintedCoins` (persisted outputs), the durability
/// mock's `submittedInputs`/`submittedOutputs` (registered entries), and its `handoffAssets`
/// (coins reserved for the peer). External dependencies (extrinsic submission, key derivation)
/// are mocked at the strategy level.
struct TransferSenderServiceTests {
    let testContext = DenominationBreakdownContext(
        unit: BigUInt(1_000_000),
        precision: 6,
        maxExponent: 7,
        minExponent: -6
    )

    let now = Date()

    let journal: CallJournal
    let mockMinter: MockCoinAllocator
    let mockDurability: MockCoinageTxService

    init() {
        let journal = CallJournal()
        self.journal = journal
        mockMinter = MockCoinAllocator()
        mockDurability = MockCoinageTxService(callJournal: journal)
    }

    // MARK: - ExactMatch Strategy Tests

    @Test("ExactMatch: coins handed off, no entry registered")
    func exactMatchContextProcessing() async throws {
        // Given: Coins that exactly match the target ($12 = $8 + $4)
        let coin1 = makeCoin(exponent: 3, derivationIndex: 1) // $8
        let coin2 = makeCoin(exponent: 2, derivationIndex: 2) // $4

        let service = makeTransferSenderService(mockDurability: mockDurability)

        // When
        let result = try await service.previewStrategy(
            amount: planks(Decimal(12)),
            availableCoins: [coin1, coin2],
            availableVouchers: [],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        // Then - Wait for handoff marks to be recorded
        try await waitForHandoff(expectedCoins: 2)

        // ExactMatch registers no entry (no extrinsic)
        #expect(mockDurability.submittedInputs.isEmpty)
        #expect(mockDurability.submittedOutputs.isEmpty)

        // Both coins are reserved for the peer
        #expect(await handedOffIndices() == Set([coin1.coin.derivationIndex, coin2.coin.derivationIndex]))
    }

    // MARK: - UnloadIntoCoins Strategy Tests

    @Test("Two voucher groups: outputs saved and recipient coins handed off")
    func twoVoucherGroupsContextProcessing() async throws {
        let voucher1 = makeVoucher(exponent: 4, derivationIndex: 1, recyclerIndex: 0) // $16
        let voucher2 = makeVoucher(exponent: 3, derivationIndex: 2, recyclerIndex: 1) // $8
        // Total: $24, need $20, change: $4

        let recyclerLoader = MockRecyclerLoader()
        let key4_0 = RecyclerKey(exponent: 4, index: 0)
        recyclerLoader.states[key4_0] = MembersPallet.RingStatus(total: 10, included: 10)
        recyclerLoader.revisions[key4_0] = 1

        let key3_1 = RecyclerKey(exponent: 3, index: 1)
        recyclerLoader.states[key3_1] = MembersPallet.RingStatus(total: 10, included: 10)
        recyclerLoader.revisions[key3_1] = 1

        let service = makeTransferSenderService(recyclerLoader: recyclerLoader, mockDurability: mockDurability)

        let result = try await service.previewStrategy(
            amount: planks(Decimal(20)),
            availableCoins: [],
            availableVouchers: [voucher1, voucher2],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        // Wait for entries to be submitted and outputs to be saved
        try await waitForSubmission(expectedEntries: 2)

        // Two groups = two entries submitted
        let allInputs = mockDurability.submittedInputs.flatMap { $0 }
        #expect(Set(allInputs.compactMap { input -> String? in
            guard case let .recyclerVoucher(idx, _) = input else { return nil }
            return "voucher:\(idx)"
        }) == Set(["voucher:\(voucher1.voucher.derivationIndex)", "voucher:\(voucher2.voucher.derivationIndex)"]))

        // Minted output coins (recipient + change) match the registered outputs exactly, and some
        // were reserved for the peer.
        let registeredOutputs = mockDurability.submittedOutputs.flatMap { $0 }
        #expect(await !(mockDurability.handoffAssets).isEmpty)
        #expect(await Set(mockMinter.mintedCoins.map(\.derivationIndex)) ==
            Set(registeredOutputs.map(\.derivationIndex)))
        #expect(await changeCoins().count == 1)
    }

    @Test("Three voucher groups: all groups processed and outputs saved")
    func threeVoucherGroupsContextProcessing() async throws {
        let voucher1 = makeVoucher(exponent: 5, derivationIndex: 1, recyclerIndex: 0) // $32
        let voucher2 = makeVoucher(exponent: 4, derivationIndex: 2, recyclerIndex: 1) // $16
        let voucher3 = makeVoucher(exponent: 3, derivationIndex: 3, recyclerIndex: 2) // $8
        // Total: $56, need $50, change: $6

        let recyclerLoader = MockRecyclerLoader()
        let key5_0 = RecyclerKey(exponent: 5, index: 0)
        recyclerLoader.states[key5_0] = MembersPallet.RingStatus(total: 10, included: 10)
        recyclerLoader.revisions[key5_0] = 1

        let key4_1 = RecyclerKey(exponent: 4, index: 1)
        recyclerLoader.states[key4_1] = MembersPallet.RingStatus(total: 10, included: 10)
        recyclerLoader.revisions[key4_1] = 1

        let key3_2 = RecyclerKey(exponent: 3, index: 2)
        recyclerLoader.states[key3_2] = MembersPallet.RingStatus(total: 10, included: 10)
        recyclerLoader.revisions[key3_2] = 1

        let service = makeTransferSenderService(recyclerLoader: recyclerLoader, mockDurability: mockDurability)

        let result = try await service.previewStrategy(
            amount: planks(Decimal(50)),
            availableCoins: [],
            availableVouchers: [voucher1, voucher2, voucher3],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        try await waitForSubmission(expectedEntries: 3)

        // Three groups = three entries submitted
        #expect((mockDurability.submittedInputs).count == 3)

        // All three vouchers registered as inputs across entries
        let allInputs = mockDurability.submittedInputs.flatMap { $0 }
        let voucherIndices = Set(allInputs.compactMap { input -> CoinageKeyIndex? in
            guard case let .recyclerVoucher(index, _) = input else { return nil }
            return index
        })
        #expect(voucherIndices == Set([voucher1, voucher2, voucher3].map(\.voucher.derivationIndex)))

        // All output coins saved (recipient + change for each group)

        // Saved coins match registered outputs exactly
        let registeredOutputs = mockDurability.submittedOutputs.flatMap { $0 }
        #expect(await Set(mockMinter.mintedCoins.map(\.derivationIndex)) ==
            Set(registeredOutputs.map(\.derivationIndex)))
        #expect(await changeCoins().count == 2)
    }

    @Test("Five voucher groups: all groups processed and submitted")
    func fiveVoucherGroupsContextProcessing() async throws {
        let voucher1 = makeVoucher(exponent: 5, derivationIndex: 1, recyclerIndex: 0) // $32
        let voucher2 = makeVoucher(exponent: 4, derivationIndex: 2, recyclerIndex: 1) // $16
        let voucher3 = makeVoucher(exponent: 3, derivationIndex: 3, recyclerIndex: 2) // $8
        let voucher4 = makeVoucher(exponent: 2, derivationIndex: 4, recyclerIndex: 3) // $4
        let voucher5 = makeVoucher(exponent: 1, derivationIndex: 5, recyclerIndex: 4) // $2
        // Total: $62, need $61, change: $1

        let recyclerLoader = MockRecyclerLoader()
        let exponents: [Int16] = [5, 4, 3, 2, 1]
        for (idx, exp) in exponents.enumerated() {
            let key = RecyclerKey(exponent: exp, index: UInt32(idx))
            recyclerLoader.states[key] = MembersPallet.RingStatus(total: 10, included: 10)
            recyclerLoader.revisions[key] = 1
        }

        let service = makeTransferSenderService(recyclerLoader: recyclerLoader, mockDurability: mockDurability)

        let result = try await service.previewStrategy(
            amount: planks(Decimal(61)),
            availableCoins: [],
            availableVouchers: [voucher1, voucher2, voucher3, voucher4, voucher5],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        try await waitForSubmission(expectedEntries: 5)

        // Five groups = five entries submitted
        #expect((mockDurability.submittedInputs).count == 5)

        // All five vouchers registered as inputs across entries
        let allInputs = mockDurability.submittedInputs.flatMap { $0 }
        let voucherIndices = Set(allInputs.compactMap { input -> CoinageKeyIndex? in
            guard case let .recyclerVoucher(index, _) = input else { return nil }
            return index
        })
        #expect(voucherIndices ==
            Set([voucher1, voucher2, voucher3, voucher4, voucher5].map(\.voucher.derivationIndex)))

        // Saved coins match registered outputs exactly
        let registeredOutputs = mockDurability.submittedOutputs.flatMap { $0 }
        #expect(await Set(mockMinter.mintedCoins.map(\.derivationIndex)) ==
            Set(registeredOutputs.map(\.derivationIndex)))
        #expect(await changeCoins().count == 1)
    }

    @Test("Multiple vouchers same recycler group: combined and submitted as one entry")
    func multipleVouchersSameGroupContextProcessing() async throws {
        let voucher1 = makeVoucher(exponent: 3, derivationIndex: 1, recyclerIndex: 0) // $8
        let voucher2 = makeVoucher(exponent: 3, derivationIndex: 2, recyclerIndex: 0) // $8, same group
        // Group total: $16, need $10, change: $6

        let recyclerLoader = MockRecyclerLoader()
        let key = RecyclerKey(exponent: 3, index: 0)
        recyclerLoader.states[key] = MembersPallet.RingStatus(total: 10, included: 10)
        recyclerLoader.revisions[key] = 1

        let service = makeTransferSenderService(recyclerLoader: recyclerLoader, mockDurability: mockDurability)

        let result = try await service.previewStrategy(
            amount: planks(Decimal(10)),
            availableCoins: [],
            availableVouchers: [voucher1, voucher2],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        try await waitForSubmission(expectedEntries: 1)

        // One group = one entry submitted
        #expect((mockDurability.submittedInputs).count == 1)

        // Both vouchers in the entry inputs
        let inputs = mockDurability.submittedInputs[0]
        #expect(Set(inputs.compactMap { input -> String? in
            guard case let .recyclerVoucher(idx, _) = input else { return nil }
            return "voucher:\(idx)"
        }) == Set(["voucher:\(voucher1.voucher.derivationIndex)", "voucher:\(voucher2.voucher.derivationIndex)"]))

        // Saved coins match registered outputs exactly
        let registeredOutputs = mockDurability.submittedOutputs.flatMap { $0 }
        #expect(await Set(mockMinter.mintedCoins.map(\.derivationIndex)) ==
            Set(registeredOutputs.map(\.derivationIndex)))
        #expect(await changeCoins().count == 2)
    }

    @Test("Voucher alone suffices: coins untouched, only the voucher is unloaded")
    func voucherOnlyWhenSufficientContextProcessing() async throws {
        let coin1 = makeCoin(exponent: 1, derivationIndex: 1) // $2
        let coin2 = makeCoin(exponent: 1, derivationIndex: 2) // $2
        let voucher = makeVoucher(exponent: 4, derivationIndex: 3, recyclerIndex: 0) // $16
        // Need $12, voucher ($16) alone is sufficient

        let recyclerLoader = MockRecyclerLoader()
        let key = RecyclerKey(exponent: 4, index: 0)
        recyclerLoader.states[key] = MembersPallet.RingStatus(total: 10, included: 10)
        recyclerLoader.revisions[key] = 1

        let service = makeTransferSenderService(recyclerLoader: recyclerLoader, mockDurability: mockDurability)

        let result = try await service.previewStrategy(
            amount: planks(Decimal(12)),
            availableCoins: [coin1, coin2],
            availableVouchers: [voucher],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        try await waitForSubmission(expectedEntries: 1)

        // Entry registered with voucher input
        #expect((mockDurability.submittedInputs).count == 1)

        // The voucher alone covers the amount, so it is the sole consumed input; the coins stay untouched.
        #expect(consumedVoucherIndices() == Set([voucher.voucher.derivationIndex]))
        #expect(consumedCoinIndices().isEmpty)

        // Saved coins match registered outputs exactly
        let registeredOutputs = mockDurability.submittedOutputs.flatMap { $0 }
        #expect(await Set(mockMinter.mintedCoins.map(\.derivationIndex)) ==
            Set(registeredOutputs.map(\.derivationIndex)))
        #expect(await changeCoins().count == 1)
    }

    @Test("Change denominations: breakdown saved and recipient handed off")
    func changeDenominationsContextProcessing() async throws {
        let voucher = makeVoucher(exponent: 4, derivationIndex: 1, recyclerIndex: 0) // $16
        // Need $13, change: $3 = $2 + $1

        let recyclerLoader = MockRecyclerLoader()
        let key = RecyclerKey(exponent: 4, index: 0)
        recyclerLoader.states[key] = MembersPallet.RingStatus(total: 10, included: 10)
        recyclerLoader.revisions[key] = 1

        let service = makeTransferSenderService(recyclerLoader: recyclerLoader, mockDurability: mockDurability)

        let result = try await service.previewStrategy(
            amount: planks(Decimal(13)),
            availableCoins: [],
            availableVouchers: [voucher],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        try await waitForSubmission(expectedEntries: 1)

        // Entry registered
        #expect((mockDurability.submittedInputs).count == 1)

        // Change coins should have exponents 1 and 0 (for $2 and $1)
        let change = await changeCoins()
        #expect(Set(change.map(\.exponent)) == Set([Int16(0), Int16(1)]))
        #expect(change.count == 2)

        // Saved coins match registered outputs exactly
        let registeredOutputs = mockDurability.submittedOutputs.flatMap { $0 }
        #expect(await Set(mockMinter.mintedCoins.map(\.derivationIndex)) ==
            Set(registeredOutputs.map(\.derivationIndex)))
    }

    @Test("Exact voucher match: outputs registered, no change needed")
    func exactVoucherMatchNoChangeContext() async throws {
        let voucher = makeVoucher(exponent: 3, derivationIndex: 1, recyclerIndex: 0) // $8

        let recyclerLoader = MockRecyclerLoader()
        let key = RecyclerKey(exponent: 3, index: 0)
        recyclerLoader.states[key] = MembersPallet.RingStatus(total: 10, included: 10)
        recyclerLoader.revisions[key] = 1

        let service = makeTransferSenderService(recyclerLoader: recyclerLoader, mockDurability: mockDurability)

        let result = try await service.previewStrategy(
            amount: planks(Decimal(8)),
            availableCoins: [],
            availableVouchers: [voucher],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        try await waitForSubmission(expectedEntries: 1)

        // Entry registered
        #expect((mockDurability.submittedInputs).count == 1)

        // Outputs registered - only the recipient coin (exact match, no change)
        let outputs = mockDurability.submittedOutputs[0]
        #expect(outputs.count == 1)
        #expect(await changeCoins().isEmpty)

        // Saved coin is the only registered output
        let registeredOutputs = mockDurability.submittedOutputs.flatMap { $0 }
        #expect(await Set(mockMinter.mintedCoins.map(\.derivationIndex)) ==
            Set(registeredOutputs.map(\.derivationIndex)))
    }

    @Test("Fractional amounts: outputs saved with fractional denominations")
    func fractionalAmountsChangeContext() async throws {
        let voucher = makeVoucher(exponent: 1, derivationIndex: 1, recyclerIndex: 0) // $2

        let recyclerLoader = MockRecyclerLoader()
        let key = RecyclerKey(exponent: 1, index: 0)
        recyclerLoader.states[key] = MembersPallet.RingStatus(total: 10, included: 10)
        recyclerLoader.revisions[key] = 1

        let service = makeTransferSenderService(recyclerLoader: recyclerLoader, mockDurability: mockDurability)

        let result = try await service.previewStrategy(
            amount: planks(Decimal(string: "1.5")!),
            availableCoins: [],
            availableVouchers: [voucher],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        try await waitForSubmission(expectedEntries: 1)

        // Entry registered
        #expect((mockDurability.submittedInputs).count == 1)

        // Outputs saved (recipient + fractional change)

        // Saved coins match registered outputs exactly
        let registeredOutputs = mockDurability.submittedOutputs.flatMap { $0 }
        #expect(await Set(mockMinter.mintedCoins.map(\.derivationIndex)) ==
            Set(registeredOutputs.map(\.derivationIndex)))
        #expect(await changeCoins().count == 1)
    }

    @Test("Larger voucher with change: multiple output coins saved")
    func largerVoucherWithChangeContext() async throws {
        // Use exponent 5 ($32) for a simpler test case
        let voucher = makeVoucher(exponent: 5, derivationIndex: 1, recyclerIndex: 0) // $32

        let recyclerLoader = MockRecyclerLoader()
        let key = RecyclerKey(exponent: 5, index: 0)
        recyclerLoader.states[key] = MembersPallet.RingStatus(total: 10, included: 10)
        recyclerLoader.revisions[key] = 1

        let service = makeTransferSenderService(recyclerLoader: recyclerLoader, mockDurability: mockDurability)

        let result = try await service.previewStrategy(
            amount: planks(Decimal(25)),
            availableCoins: [],
            availableVouchers: [voucher],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        try await waitForSubmission(expectedEntries: 1)

        // Entry registered
        #expect((mockDurability.submittedInputs).count == 1)

        // Outputs saved: recipient ($25) + change ($7 = $4 + $2 + $1)
        // Change coins must have exact exponents [0, 1, 2] for $1, $2, $4
        let change = await changeCoins()
        #expect(change.map(\.exponent).sorted() == [Int16(0), Int16(1), Int16(2)])
        #expect(change.count == 3)

        // Saved coins match registered outputs exactly
        let registeredOutputs = mockDurability.submittedOutputs.flatMap { $0 }
        #expect(await Set(mockMinter.mintedCoins.map(\.derivationIndex)) ==
            Set(registeredOutputs.map(\.derivationIndex)))
    }

    // MARK: - SplitCoin Strategy Tests

    @Test("Split strategy: overflow coin consumed, outputs saved and recipient handed off")
    func splitStrategyContextProcessing() async throws {
        // Given: $8 coin, need $5, should split into $5 recipient + $3 change
        let coin = makeCoin(exponent: 3, derivationIndex: 1) // $8

        let service = makeTransferSenderService(mockDurability: mockDurability)

        // When
        let result = try await service.previewStrategy(
            amount: planks(Decimal(5)),
            availableCoins: [coin],
            availableVouchers: [],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        // Then
        try await waitForSubmission(expectedEntries: 1)

        // Entry registered with overflow coin input
        let inputs = mockDurability.submittedInputs[0]
        #expect(inputs.contains { input in
            guard case let .coin(.own(idx, _)) = input else { return false }
            return idx == coin.coin.derivationIndex
        })

        // Outputs saved (recipient split into 2 coins + change $3 = $2 + $1)
        let outputs = mockDurability.submittedOutputs[0]
        #expect(outputs.count == 4)

        // Change coins must have exponents [0, 1] for $1 and $2
        let change = await changeCoins()
        #expect(change.map(\.exponent).sorted() == [Int16(0), Int16(1)])
        #expect(change.count == 2)

        // Saved coins match registered outputs exactly
        let registeredOutputs = mockDurability.submittedOutputs.flatMap { $0 }
        #expect(await Set(mockMinter.mintedCoins.map(\.derivationIndex)) ==
            Set(registeredOutputs.map(\.derivationIndex)))
    }

    @Test("Split with whole coins: whole coins handed off without splitting, overflow consumed")
    func splitWithWholeCoinsContextProcessing() async throws {
        // Given: $8 + $4 + $2 coins, need $11
        // Should use $8 + $4 whole ($12 total), split overflow coin from second $4 for $3 + $1 change
        let coin1 = makeCoin(exponent: 3, derivationIndex: 1) // $8
        let coin2 = makeCoin(exponent: 2, derivationIndex: 2) // $4
        let coin3 = makeCoin(exponent: 1, derivationIndex: 3) // $2

        let service = makeTransferSenderService(mockDurability: mockDurability)

        // When
        let result = try await service.previewStrategy(
            amount: planks(Decimal(11)),
            availableCoins: [coin1, coin2, coin3],
            availableVouchers: [],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        // Then - Split registers the overflow coin as consumed
        try await waitForSubmission(expectedEntries: 1)

        // Entry registered with only overflow coin input (coin2)
        let inputs = mockDurability.submittedInputs[0]
        #expect(inputs.contains { input in
            guard case let .coin(.own(idx, _)) = input else { return false }
            return idx == coin2.coin.derivationIndex
        })

        // Whole coins (coin1) handed off without consumption
        #expect(await handedOffIndices().contains(coin1.coin.derivationIndex))

        // coin3 not used at all — neither consumed nor handed off
        #expect(!consumedCoinIndices().contains(coin3.coin.derivationIndex))
        #expect(await !handedOffIndices().contains(coin3.coin.derivationIndex))

        // Outputs saved (recipient $11 + change $1)
        // Saved coins match registered outputs exactly
        let registeredOutputs = mockDurability.submittedOutputs.flatMap { $0 }
        #expect(await Set(mockMinter.mintedCoins.map(\.derivationIndex)) ==
            Set(registeredOutputs.map(\.derivationIndex)))
        #expect(await changeCoins().count == 1)
    }

    // MARK: - Ordering and Error Handling Tests

    @Test("UnloadIntoCoins: reservation precedes scheduling, and prepare submits nothing")
    func registrationPrecedesReservationForUnload() async throws {
        let voucher1 = makeVoucher(exponent: 4, derivationIndex: 1, recyclerIndex: 0) // $16
        let voucher2 = makeVoucher(exponent: 3, derivationIndex: 2, recyclerIndex: 1) // $8
        // Total: $24, need $20, change: $4

        let recyclerLoader = MockRecyclerLoader()
        let key4_0 = RecyclerKey(exponent: 4, index: 0)
        recyclerLoader.states[key4_0] = MembersPallet.RingStatus(total: 10, included: 10)
        recyclerLoader.revisions[key4_0] = 1

        let key3_1 = RecyclerKey(exponent: 3, index: 1)
        recyclerLoader.states[key3_1] = MembersPallet.RingStatus(total: 10, included: 10)
        recyclerLoader.revisions[key3_1] = 1

        let service = makeTransferSenderService(recyclerLoader: recyclerLoader, mockDurability: mockDurability)

        let result = try await service.previewStrategy(
            amount: planks(Decimal(20)),
            availableCoins: [],
            availableVouchers: [voucher1, voucher2],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        try await waitForSubmission(expectedEntries: 2)

        let events = journal.events

        let handoffIdx = try #require(
            events.firstIndex(of: "preCommitHandoff"),
            "no handoff reservation recorded"
        )
        let scheduleIdx = try #require(
            events.firstIndex(of: "schedule"),
            "no scheduling recorded — the assertion below would be vacuous"
        )

        // Nothing is submitted while the memo is being prepared: the transactions are declared and
        // only registered when the transport commits them.
        #expect(!events.contains("submit"), "prepare must not submit: \(events)")

        // The reservation is what holds the coins in the window before the transactions exist, so it
        // must already be in place by the time they are registered.
        #expect(
            handoffIdx < scheduleIdx,
            "handoff reservation must precede scheduling: \(events)"
        )
    }

    @Test("SplitCoin: reservation precedes scheduling, and prepare submits nothing")
    func registrationPrecedesReservationForSplit() async throws {
        let coin = makeCoin(exponent: 3, derivationIndex: 1) // $8

        let service = makeTransferSenderService(mockDurability: mockDurability)

        let result = try await service.previewStrategy(
            amount: planks(Decimal(5)),
            availableCoins: [coin],
            availableVouchers: [],
            breakdownContext: testContext
        )
        let prepared = try await service.execute(
            result: result,
            breakdownContext: testContext,
            groupId: "transfer"
        )
        // Declared work only becomes real when the transport commits it.
        try prepared.commit(in: InMemoryRegistrationScope())

        try await waitForSubmission(expectedEntries: 1)

        let events = journal.events

        let handoffIdx = try #require(
            events.firstIndex(of: "preCommitHandoff"),
            "no handoff reservation recorded"
        )
        let scheduleIdx = try #require(
            events.firstIndex(of: "schedule"),
            "no scheduling recorded — the assertion below would be vacuous"
        )

        // Nothing is submitted while the memo is being prepared: the transactions are declared and
        // only registered when the transport commits them.
        #expect(!events.contains("submit"), "prepare must not submit: \(events)")

        // The reservation is what holds the coins in the window before the transactions exist, so it
        // must already be in place by the time they are registered.
        #expect(
            handoffIdx < scheduleIdx,
            "handoff reservation must precede scheduling: \(events)"
        )
    }
}

extension TransferSenderServiceTests {
    // MARK: - Helpers

    /// Derivation indices of the coins the strategy reserved for the peer via `preCommitHandoff`.
    private func handedOffIndices() async -> Set<CoinageKeyIndex> {
        await Set(mockDurability.handoffAssets.map(\.derivationIndex))
    }

    /// Minted output coins that were kept (not handed off) — i.e. change.
    private func changeCoins() async -> [Coin] {
        let handedOff = await handedOffIndices()
        return await mockMinter.mintedCoins.filter { !handedOff.contains($0.derivationIndex) }
    }

    /// Derivation indices of own coins consumed as durability entry inputs.
    private func consumedCoinIndices() -> Set<CoinageKeyIndex> {
        let inputs = mockDurability.submittedInputs.flatMap { $0 }
        return Set(inputs.compactMap { input -> CoinageKeyIndex? in
            guard case let .coin(.own(index, _)) = input else { return nil }
            return index
        })
    }

    /// Derivation indices of vouchers consumed as durability entry inputs.
    private func consumedVoucherIndices() -> Set<CoinageKeyIndex> {
        let inputs = mockDurability.submittedInputs.flatMap { $0 }
        return Set(inputs.compactMap { input -> CoinageKeyIndex? in
            guard case let .recyclerVoucher(index, _) = input else { return nil }
            return index
        })
    }

    private func waitForHandoff(
        expectedCoins: Int = 0,
        timeout: TimeInterval = 2.0
    ) async throws {
        let start = Date()
        while Date().timeIntervalSince(start) < timeout {
            if await mockDurability.handoffAssets.count >= expectedCoins {
                return
            }

            try await Task.sleep(for: .milliseconds(20))
        }

        let got = await mockDurability.handoffAssets.count
        throw TestError.timeout("waitForHandoff: expected \(expectedCoins) handed off assets, got \(got)")
    }

    private func waitForSubmission(
        expectedEntries: Int = 0,
        timeout: TimeInterval = 2.0
    ) async throws {
        let start = Date()
        while Date().timeIntervalSince(start) < timeout {
            // `prepare` declares rather than submits now, so the transactions it produced are the
            // scheduled ones; a strategy that puts nothing on chain still declares nothing.
            let actualEntries = mockDurability.submittedInputs.count
                + mockDurability.scheduledRequests.count

            if actualEntries >= expectedEntries {
                return
            }

            try await Task.sleep(for: .milliseconds(20))
        }

        let actualEntries = mockDurability.submittedInputs.count
            + mockDurability.scheduledRequests.count
        throw TestError.timeout("waitForSubmission: expected \(expectedEntries) entries, got \(actualEntries)")
    }

    private func planks(_ decimal: Decimal) -> BigUInt {
        decimal.toSubstrateAmount(precision: testContext.precision)!
    }

    private func makeCoin(
        exponent: Int16,
        derivationIndex: CoinageKeyIndex = 0,
        age: Int16 = 0
    ) -> TrackedCoin {
        let coin = Coin(
            exponent: exponent,
            derivationIndex: derivationIndex,
            age: age,
            isOnchain: true,
            publicKey: Data(repeating: UInt8(truncatingIfNeeded: derivationIndex.item), count: 32)
        )
        return TrackedCoin(
            coin: coin,
            state: CoinageAssetState(handedOff: false, consumerStatus: nil, minterStatus: nil)
        )
    }

    private func makeVoucher(
        exponent: Int16,
        derivationIndex: CoinageKeyIndex,
        recyclerIndex: UInt32 = 0,
        readyAt: Date = Date.distantPast
    ) -> TrackedVoucher {
        let voucher = Voucher(
            exponent: exponent,
            derivationIndex: derivationIndex,
            allocatedAt: Date.distantPast,
            readyAt: readyAt,
            remoteState: .inRecycler(.init(index: recyclerIndex, membersCount: 0)),
            publicKey: Data(repeating: UInt8(truncatingIfNeeded: derivationIndex.item), count: 32)
        )
        return TrackedVoucher(
            voucher: voucher,
            state: CoinageAssetState(handedOff: false, consumerStatus: nil, minterStatus: nil)
        )
    }

    private func makeTransferSenderService(
        originFactory _: StubOriginFactory = StubOriginFactory(),
        recyclerLoader: MockRecyclerLoader = MockRecyclerLoader(),
        blockInfoProvider _: MockBlockNumberProvider = MockBlockNumberProvider(),
        mockDurability: MockCoinageTxService = MockCoinageTxService()
    ) -> TransferSenderService {
        let coinSelector = CoinSelector()
        let memoBuilder = MockMemoBuilder()

        let planFactory = TransferPlanFactory(
            minter: mockMinter,
            durability: mockDurability,
            dateProvider: StubDateProvider(now),
            logger: nil
        )

        return TransferSenderService(
            coinSelector: coinSelector,
            planFactory: planFactory,
            memoBuilder: memoBuilder,
            recyclerLoader: recyclerLoader,
            txService: mockDurability,
            logger: nil
        )
    }
}

enum TestError: Error {
    case timeout(String)
}
