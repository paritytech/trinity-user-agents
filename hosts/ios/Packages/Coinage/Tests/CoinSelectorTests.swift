import Testing
import Foundation
import BigInt
@testable import Coinage

struct CoinSelectorTests {
    // MARK: - Mock RecyclerReadinessService

    // MARK: - Test Setup

    private let testContext = DenominationBreakdownContext(
        unit: BigUInt(1_000_000),
        precision: 6,
        maxExponent: 7,
        minExponent: -6
    )

    private let limits = UnloadCallLimits(maxVouchersPerCall: 10, maxOutputsPerCall: 32)

    private func makeSelector() -> CoinSelector {
        CoinSelector()
    }

    private func makeCoin(
        exponent: Int16,
        age: Int16 = 0,
        derivationIndex: CoinageKeyIndex = 0,
        selectable: Bool = true
    ) -> TrackedCoin {
        let coin = Coin(
            exponent: exponent,
            derivationIndex: derivationIndex,
            age: age,
            isOnchain: true,
            publicKey: Data(repeating: UInt8(truncatingIfNeeded: derivationIndex.item), count: 32)
        )
        let state = selectable
            ? CoinageAssetState(handedOff: false, consumerStatus: nil, minterStatus: nil)
            : CoinageAssetState(handedOff: false, consumerStatus: .finalizedSuccess, minterStatus: nil)
        return TrackedCoin(coin: coin, state: state)
    }

    private func makeVoucher(
        exponent: Int16,
        derivationIndex: CoinageKeyIndex = 0,
        readyAt: Date = Date.distantPast
    ) -> TrackedVoucher {
        let voucher = Voucher(
            exponent: exponent,
            derivationIndex: derivationIndex,
            allocatedAt: Date.distantPast,
            readyAt: readyAt,
            remoteState: .inRecycler(.init(index: 0, membersCount: 0)),
            publicKey: Data(repeating: UInt8(truncatingIfNeeded: derivationIndex.item), count: 32)
        )
        return TrackedVoucher(
            voucher: voucher,
            state: CoinageAssetState(handedOff: false, consumerStatus: nil, minterStatus: nil)
        )
    }

    private func total(_ denominations: [Denomination]) -> Decimal {
        denominations.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
    }

    private func outputCount(_ allocation: RecyclerGroupAllocation) -> Int {
        allocation.recipientDenominations.count + allocation.changeDenominations.count
    }

    private let now = Date()

    private func planks(_ decimal: Decimal) -> BigUInt {
        decimal.toSubstrateAmount(precision: testContext.precision)!
    }

    // MARK: - Strategy 1: Exact Match

    @Test("Exact match with single coin")
    func exactMatchSingleCoin() async throws {
        let coins = [makeCoin(exponent: 3)] // $8

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(8)),
            coins: coins,
            vouchers: [],
            breakdownContext: testContext,
            limits: limits
        ))

        switch result {
        case let .exactMatch(selectedCoins):
            #expect(selectedCoins.count == 1)
            #expect(selectedCoins[0].exponent == 3)
        default:
            Issue.record("Expected exactMatch, got \(result)")
        }
    }

    @Test("Exact match with multiple coins")
    func exactMatchMultipleCoins() async throws {
        let coins = [
            makeCoin(exponent: 3, derivationIndex: 1), // $8
            makeCoin(exponent: 2, derivationIndex: 2) // $4
        ]

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(12)),
            coins: coins,
            vouchers: [],
            breakdownContext: testContext,
            limits: limits
        ))

        switch result {
        case let .exactMatch(selectedCoins):
            let sum = selectedCoins
                .reduce(Decimal(0)) { $0 + testContext.amount(for: Denomination(exponent: $1.exponent)) }
            #expect(sum == Decimal(12))
        default:
            Issue.record("Expected exactMatch, got \(result)")
        }
    }

    @Test("Prefers fewer coins for exact match")
    func prefersFewerCoinsForExactMatch() async throws {
        let coins = [
            makeCoin(exponent: 2, derivationIndex: 2), // $4
            makeCoin(exponent: 3, derivationIndex: 1), // $8
            makeCoin(exponent: 2, derivationIndex: 3) // $4
        ]

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(8)),
            coins: coins,
            vouchers: [],
            breakdownContext: testContext,
            limits: limits
        ))

        switch result {
        case let .exactMatch(selectedCoins):
            #expect(selectedCoins.count == 1)
            #expect(selectedCoins[0].exponent == 3) // Should pick $8, not $4+$4
        default:
            Issue.record("Expected exactMatch, got \(result)")
        }
    }

    // MARK: - Strategy 2: Split

    @Test("Split when no exact match")
    func splitWhenNoExactMatch() async throws {
        let coins = [makeCoin(exponent: 4)] // $16

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(7)),
            coins: coins,
            vouchers: [],
            breakdownContext: testContext,
            limits: limits
        ))

        switch result {
        case let .split(wholeCoins, overflowCoin, targetDenoms, changeDenoms):
            #expect(wholeCoins.isEmpty) // Single coin, no whole coins
            #expect(overflowCoin.exponent == 4)
            // $7 = $4 + $2 + $1
            let targetSum = targetDenoms.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
            #expect(targetSum == Decimal(7))
            // Change = $16 - $7 = $9 = $8 + $1
            let changeSum = changeDenoms.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
            #expect(changeSum == Decimal(9))
        default:
            Issue.record("Expected split, got \(result)")
        }
    }

    @Test("Uses smallest sufficient coin for split")
    func usesSmallestSufficientCoinForSplit() async throws {
        let coins = [
            makeCoin(exponent: 7, derivationIndex: 1), // $128
            makeCoin(exponent: 4, derivationIndex: 2), // $16
            makeCoin(exponent: 3, derivationIndex: 3) // $8
        ]

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(10)),
            coins: coins,
            vouchers: [],
            breakdownContext: testContext,
            limits: limits
        ))

        switch result {
        case let .split(wholeCoins, overflowCoin, _, _):
            // When a single sufficient coin exists, use it directly (no whole coins)
            #expect(wholeCoins.isEmpty)
            #expect(overflowCoin.exponent == 4) // $16 is smallest coin > $10
        default:
            Issue.record("Expected split, got \(result)")
        }
    }

    @Test("Calculates correct target and change denominations")
    func calculatesCorrectDenominations() async throws {
        let coins = [makeCoin(exponent: 5)] // $32

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(12)),
            coins: coins,
            vouchers: [],
            breakdownContext: testContext,
            limits: limits
        ))

        switch result {
        case let .split(wholeCoins, _, targetDenoms, changeDenoms):
            #expect(wholeCoins.isEmpty) // Single coin case
            // $12 = $8 + $4
            #expect(targetDenoms.count == 2)
            #expect(targetDenoms[0].exponent == 3) // $8
            #expect(targetDenoms[1].exponent == 2) // $4

            // Change = $32 - $12 = $20 = $16 + $4
            #expect(changeDenoms.count == 2)
            #expect(changeDenoms[0].exponent == 4) // $16
            #expect(changeDenoms[1].exponent == 2) // $4
        default:
            Issue.record("Expected split, got \(result)")
        }
    }

    @Test("Multi-coin split uses whole coins plus overflow coin")
    func multiCoinSplitUsesWholeCoinsAndOverflow() async throws {
        // Scenario: need $10, have $8 + $4
        // Should use $8 whole, split $4 for remaining $2
        let coins = [
            makeCoin(exponent: 3, derivationIndex: 1), // $8
            makeCoin(exponent: 2, derivationIndex: 2) // $4
        ]

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(10)),
            coins: coins,
            vouchers: [],
            breakdownContext: testContext,
            limits: limits
        ))

        switch result {
        case let .split(wholeCoins, overflowCoin, targetDenoms, changeDenoms):
            // $8 should be used whole
            #expect(wholeCoins.count == 1)
            #expect(wholeCoins[0].exponent == 3)

            // $4 should be the overflow coin (split for remaining $2)
            #expect(overflowCoin.exponent == 2)

            // Target: $2 from the split
            let targetSum = targetDenoms.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
            #expect(targetSum == Decimal(2))

            // Change: $4 - $2 = $2
            let changeSum = changeDenoms.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
            #expect(changeSum == Decimal(2))
        default:
            Issue.record("Expected split with whole coins, got \(result)")
        }
    }

    @Test("Multi-coin split minimizes coin count")
    func multiCoinSplitMinimizesCoinCount() async throws {
        // Scenario: need $15, have $8 + $4 + $4 + $2
        // Greedy largest-first: $8 (whole) + $4 (whole) = $12, then $4 overflow (need $3)
        // NOT $8 + $4 + $2 + $4 which uses more coins
        let coins = [
            makeCoin(exponent: 3, derivationIndex: 1), // $8
            makeCoin(exponent: 2, derivationIndex: 2), // $4
            makeCoin(exponent: 2, derivationIndex: 3), // $4
            makeCoin(exponent: 1, derivationIndex: 4) // $2
        ]

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(15)),
            coins: coins,
            vouchers: [],
            breakdownContext: testContext,
            limits: limits
        ))

        switch result {
        case let .split(wholeCoins, overflowCoin, targetDenoms, changeDenoms):
            // Should use $8 + $4 whole (greedy largest first)
            #expect(wholeCoins.count == 2)
            let wholeSum = wholeCoins
                .reduce(Decimal(0)) { $0 + testContext.amount(for: Denomination(exponent: $1.exponent)) }
            #expect(wholeSum == Decimal(12)) // $8 + $4

            // Overflow coin should be the next $4
            #expect(overflowCoin.exponent == 2)

            // Target from split: $15 - $12 = $3
            let targetSum = targetDenoms.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
            #expect(targetSum == Decimal(3))

            // Change: $4 - $3 = $1
            let changeSum = changeDenoms.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
            #expect(changeSum == Decimal(1))
        default:
            Issue.record("Expected split with minimum coin count, got \(result)")
        }
    }

    // MARK: - Strategy 3: Unload Into Coins (unified voucher strategy)

    @Test("Unload ready voucher for exact match")
    func unloadReadyVoucherExactMatch() async throws {
        let vouchers = [makeVoucher(exponent: 3, readyAt: Date.distantPast)] // $8, ready

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(8)),
            coins: [],
            vouchers: vouchers,
            breakdownContext: testContext,
            limits: limits
        ))

        // Now uses unified unloadIntoCoins case (empty coins array for pure unload)
        switch result {
        case let .unloadIntoCoins(coins, perGroupAllocations):
            #expect(coins.isEmpty) // Pure unload, no coins used
            let selectedVouchers = perGroupAllocations.flatMap(\.vouchers)
            #expect(selectedVouchers.count == 1)
            #expect(selectedVouchers[0].exponent == 3)
            // Recipient denominations for $8
            let recipientDenoms = perGroupAllocations.flatMap(\.recipientDenominations)
            let recipientSum = recipientDenoms.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
            #expect(recipientSum == Decimal(8))
            // No change for exact match
            let changeDenoms = perGroupAllocations.flatMap(\.changeDenominations)
            #expect(changeDenoms.isEmpty)
        default:
            Issue.record("Expected unloadIntoCoins, got \(result)")
        }
    }

    // MARK: - Strategy 3: Coins Plus Unload (now unified in unloadIntoCoins)

    @Test("Combines coins and voucher when beneficial")
    func combinesCoinsAndVoucher() async throws {
        let coins = [makeCoin(exponent: 2)] // $4
        let vouchers = [makeVoucher(exponent: 3, readyAt: Date.distantPast)] // $8

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(12)), // $8 + $4
            coins: coins,
            vouchers: vouchers,
            breakdownContext: testContext,
            limits: limits
        ))

        // Now uses unified unloadIntoCoins case with coins + vouchers
        switch result {
        case let .unloadIntoCoins(selectedCoins, perGroupAllocations):
            let coinSum = selectedCoins
                .reduce(Decimal(0)) { $0 + testContext.amount(for: Denomination(exponent: $1.exponent)) }
            let selectedVouchers = perGroupAllocations.flatMap(\.vouchers)
            let voucherSum = selectedVouchers
                .reduce(Decimal(0)) { $0 + testContext.amount(for: Denomination(exponent: $1.exponent)) }
            #expect(coinSum + voucherSum >= Decimal(12))
            // Recipient denominations cover only the voucher-funded portion (amount - coinContribution)
            let recipientDenoms = perGroupAllocations.flatMap(\.recipientDenominations)
            let recipientSum = recipientDenoms.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
            let needed = Decimal(12) - coinSum
            #expect(recipientSum == needed)
            // Change = voucherSum - needed (overshoot from vouchers only)
            let changeDenoms = perGroupAllocations.flatMap(\.changeDenominations)
            let changeSum = changeDenoms.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
            #expect(changeSum == voucherSum - needed)
            // Total sent to recipient: existing coins + new coins from unload
            #expect(coinSum + recipientSum == Decimal(12))
        default:
            Issue.record("Expected unloadIntoCoins, got \(result)")
        }
    }

    // MARK: - Strategy 3: Unload with Change (formerly Unload and Split)

    @Test("Unload with change when no exact voucher match")
    func unloadWithChangeNoExactMatch() async throws {
        let vouchers = [makeVoucher(exponent: 4, readyAt: Date.distantPast)] // $16

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(10)),
            coins: [],
            vouchers: vouchers,
            breakdownContext: testContext,
            limits: limits
        ))

        // Now uses unified unloadIntoCoins case with change denominations
        switch result {
        case let .unloadIntoCoins(coins, perGroupAllocations):
            #expect(coins.isEmpty) // Pure unload
            let selectedVouchers = perGroupAllocations.flatMap(\.vouchers)
            #expect(selectedVouchers.count == 1)
            #expect(selectedVouchers[0].exponent == 4)
            // $10 = $8 + $2
            let recipientDenoms = perGroupAllocations.flatMap(\.recipientDenominations)
            let recipientSum = recipientDenoms.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
            #expect(recipientSum == Decimal(10))
            // Change = $16 - $10 = $6 = $4 + $2
            let changeDenoms = perGroupAllocations.flatMap(\.changeDenominations)
            let changeSum = changeDenoms.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
            #expect(changeSum == Decimal(6))
        default:
            Issue.record("Expected unloadIntoCoins, got \(result)")
        }
    }

    // MARK: - Strategy 3: Multiple Vouchers (now unified in unloadIntoCoins)

    @Test("Multiple vouchers combined when needed")
    func multipleVouchersCombined() async throws {
        // No coins, vouchers don't sum exactly
        let vouchers = [
            makeVoucher(exponent: 3, derivationIndex: 1, readyAt: Date.distantPast), // $8
            makeVoucher(exponent: 2, derivationIndex: 2, readyAt: Date.distantPast), // $4
            makeVoucher(exponent: 1, derivationIndex: 3, readyAt: Date.distantPast) // $2
        ]

        // Target $13 - no exact combo, but $8 + $4 + $2 = $14 >= $13
        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(13)),
            coins: [],
            vouchers: vouchers,
            breakdownContext: testContext,
            limits: limits
        ))

        // Now uses unified unloadIntoCoins case
        switch result {
        case let .unloadIntoCoins(coins, perGroupAllocations):
            #expect(coins.isEmpty) // Pure unload
            let selectedVouchers = perGroupAllocations.flatMap(\.vouchers)
            let voucherSum = selectedVouchers
                .reduce(Decimal(0)) { $0 + testContext.amount(for: Denomination(exponent: $1.exponent)) }
            #expect(voucherSum >= Decimal(13))
            // Recipient denominations for $13
            let recipientDenoms = perGroupAllocations.flatMap(\.recipientDenominations)
            let recipientSum = recipientDenoms.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
            #expect(recipientSum == Decimal(13))
            // Change accounts for overage ($14 - $13 = $1)
            let changeDenoms = perGroupAllocations.flatMap(\.changeDenominations)
            let changeSum = changeDenoms.reduce(Decimal(0)) { $0 + testContext.amount(for: $1) }
            #expect(voucherSum - changeSum == Decimal(13))
        default:
            Issue.record("Expected unloadIntoCoins, got \(result)")
        }
    }

    // MARK: - Error Cases

    @Test("Zero amount returns error")
    func zeroAmountReturnsError() async throws {
        let coins = [makeCoin(exponent: 3)]

        do {
            _ = try await makeSelector().selectCoins(SelectCoinsInput(
                amount: BigUInt(0),
                coins: coins,
                vouchers: [],
                breakdownContext: testContext,
                limits: limits
            ))
            Issue.record("Expected zeroAmount error")
        } catch let error as CoinSelectionError {
            #expect(error == .zeroAmount)
        }
    }

    @Test("Empty wallet returns error")
    func emptyWalletReturnsError() async throws {
        do {
            _ = try await makeSelector().selectCoins(SelectCoinsInput(
                amount: planks(Decimal(8)),
                coins: [],
                vouchers: [],
                breakdownContext: testContext,
                limits: limits
            ))
            Issue.record("Expected emptyWallet error")
        } catch let error as CoinSelectionError {
            #expect(error == .emptyWallet)
        }
    }

    @Test("Insufficient funds returns error")
    func insufficientFundsReturnsError() async throws {
        let coins = [makeCoin(exponent: 2)] // $4

        do {
            _ = try await makeSelector().selectCoins(SelectCoinsInput(
                amount: planks(Decimal(100)),
                coins: coins,
                vouchers: [],
                breakdownContext: testContext,
                limits: limits
            ))
            Issue.record("Expected insufficientFunds error")
        } catch let error as CoinSelectionError {
            #expect(error == .insufficientFunds)
        }
    }

    // MARK: - Strategy Priority Order

    @Test("Prefers exact match over split")
    func prefersExactMatchOverSplit() async throws {
        let coins = [
            makeCoin(exponent: 3, derivationIndex: 1), // $8
            makeCoin(exponent: 4, derivationIndex: 2) // $16
        ]

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(8)),
            coins: coins,
            vouchers: [],
            breakdownContext: testContext,
            limits: limits
        ))

        // Should pick exact match ($8), not split ($16)
        switch result {
        case let .exactMatch(selectedCoins):
            #expect(selectedCoins.count == 1)
            #expect(selectedCoins[0].exponent == 3)
        default:
            Issue.record("Expected exactMatch (not split), got \(result)")
        }
    }

    @Test("Prefers split over voucher unload")
    func prefersSplitOverVoucherUnload() async throws {
        let coins = [makeCoin(exponent: 4)] // $16
        let vouchers = [makeVoucher(exponent: 3, readyAt: Date.distantPast)] // $8, ready

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(8)),
            coins: coins,
            vouchers: vouchers,
            breakdownContext: testContext,
            limits: limits
        ))

        // Should prefer split (1 tx, 0 tokens) over unload (1 tx, 1 token)
        switch result {
        case .split:
            break // Expected
        default:
            Issue.record("Expected split (not unload), got \(result)")
        }
    }

    // MARK: - Spent Coins Handling

    @Test("Ignores spent coins")
    func ignoresSpentCoins() async throws {
        let coins = [
            makeCoin(exponent: 3, derivationIndex: 1, selectable: false), // $8, spent
            makeCoin(exponent: 4, derivationIndex: 2, selectable: true) // $16, available
        ]

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(8)),
            coins: coins,
            vouchers: [],
            breakdownContext: testContext,
            limits: limits
        ))

        // Should split the $16 since $8 is spent
        switch result {
        case let .split(wholeCoins, overflowCoin, _, _):
            #expect(wholeCoins.isEmpty) // No whole coins (only $16 available)
            #expect(overflowCoin.exponent == 4)
        default:
            Issue.record("Expected split (ignoring spent $8), got \(result)")
        }
    }

    // MARK: - Fractional Values

    @Test("Handles fractional amounts")
    func handlesFractionalAmounts() async throws {
        let coins = [makeCoin(exponent: -1)] // $0.5

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(string: "0.5")!),
            coins: coins,
            vouchers: [],
            breakdownContext: testContext,
            limits: limits
        ))

        switch result {
        case let .exactMatch(selectedCoins):
            #expect(selectedCoins.count == 1)
            #expect(selectedCoins[0].exponent == -1)
        default:
            Issue.record("Expected exactMatch for fractional, got \(result)")
        }
    }

    // MARK: - Power-of-Two Constraint Removed

    @Test("No power-of-two voucher count constraint")
    func noPowerOfTwoConstraint() async throws {
        // Create 3 vouchers (not a power of two)
        let vouchers = [
            makeVoucher(exponent: 2, derivationIndex: 1, readyAt: Date.distantPast), // $4
            makeVoucher(exponent: 2, derivationIndex: 2, readyAt: Date.distantPast), // $4
            makeVoucher(exponent: 2, derivationIndex: 3, readyAt: Date.distantPast) // $4
        ]

        // Target $11 - needs at least 3 vouchers ($4 + $4 + $4 = $12)
        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(11)),
            coins: [],
            vouchers: vouchers,
            breakdownContext: testContext,
            limits: limits
        ))

        // Should succeed with 3 vouchers (was blocked by power-of-two constraint before)
        switch result {
        case let .unloadIntoCoins(_, perGroupAllocations):
            let selectedVouchers = perGroupAllocations.flatMap(\.vouchers)
            // Should use all 3 vouchers
            #expect(selectedVouchers.count >= 2) // At minimum need 3 to cover $11
            let voucherSum = selectedVouchers
                .reduce(Decimal(0)) { $0 + testContext.amount(for: Denomination(exponent: $1.exponent)) }
            #expect(voucherSum >= Decimal(11))
        default:
            Issue.record("Expected unloadIntoCoins with non-power-of-two voucher count, got \(result)")
        }
    }

    // MARK: - Voucher Unload

    @Test("A selectable in-recycler voucher is unloaded to cover the amount")
    func selectableVoucherUnloaded() async throws {
        let vouchers = [
            makeVoucher(exponent: 3, derivationIndex: 0) // $8
        ]

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(8)),
            coins: [],
            vouchers: vouchers,
            breakdownContext: testContext,
            limits: limits
        ))

        if case let .unloadIntoCoins(_, perGroupAllocations) = result {
            #expect(perGroupAllocations.flatMap(\.vouchers).map(\.derivationIndex) == [0])
        } else {
            Issue.record("Expected unloadIntoCoins, got \(result)")
        }
    }

    // MARK: - Pallet Call Limits

    @Test("A recycler group larger than the pallet limit is unloaded by several calls")
    func oversizedRecyclerGroupIsSplitAcrossCalls() async throws {
        let vouchers = (0 ..< 5).map { makeVoucher(exponent: 0, derivationIndex: .harness(UInt64($0))) }

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(5)),
            coins: [],
            vouchers: vouchers,
            breakdownContext: testContext,
            limits: UnloadCallLimits(maxVouchersPerCall: 2, maxOutputsPerCall: 32)
        ))

        guard case let .unloadIntoCoins(_, allocations) = result else {
            Issue.record("Expected unloadIntoCoins, got \(result)")
            return
        }

        #expect(allocations.map(\.vouchers.count) == [2, 2, 1])
        #expect(allocations.allSatisfy { $0.recyclerKey == RecyclerKey(exponent: 0, index: 0) })
        #expect(allocations.reduce(Decimal(0)) { $0 + total($1.recipientDenominations) } == Decimal(5))
    }

    @Test("A call that would mint more coins than the pallet allows is split again")
    func oversizedOutputSetIsSplitAgain() async throws {
        // Exponent 7 is the context maximum, so two vouchers cannot consolidate into a single coin.
        let vouchers = [
            makeVoucher(exponent: 7, derivationIndex: 0),
            makeVoucher(exponent: 7, derivationIndex: 1)
        ]

        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(256)),
            coins: [],
            vouchers: vouchers,
            breakdownContext: testContext,
            limits: UnloadCallLimits(maxVouchersPerCall: 2, maxOutputsPerCall: 1)
        ))

        guard case let .unloadIntoCoins(_, allocations) = result else {
            Issue.record("Expected unloadIntoCoins, got \(result)")
            return
        }

        #expect(allocations.map(\.vouchers.count) == [1, 1])
        #expect(allocations.allSatisfy { outputCount($0) <= 1 })
    }

    @Test("The call carrying both recipient and change is split when its coins overflow the limit")
    func mixedRecipientAndChangeCallIsSplit() async throws {
        let vouchers = [
            makeVoucher(exponent: 7, derivationIndex: 0),
            makeVoucher(exponent: 7, derivationIndex: 1)
        ]

        // $129 recipient + $127 change out of one $256 call needs 2 + 7 coins: within the two-voucher
        // count limit, over the eight-coin output limit.
        let result = try await makeSelector().selectCoins(SelectCoinsInput(
            amount: planks(Decimal(129)),
            coins: [],
            vouchers: vouchers,
            breakdownContext: testContext,
            limits: UnloadCallLimits(maxVouchersPerCall: 2, maxOutputsPerCall: 8)
        ))

        guard case let .unloadIntoCoins(_, allocations) = result else {
            Issue.record("Expected unloadIntoCoins, got \(result)")
            return
        }

        #expect(allocations.map(\.vouchers.count) == [1, 1])
        #expect(allocations.allSatisfy { outputCount($0) <= 8 })
        #expect(allocations.reduce(Decimal(0)) { $0 + total($1.recipientDenominations) } == Decimal(129))
        #expect(allocations.reduce(Decimal(0)) { $0 + total($1.changeDenominations) } == Decimal(127))
    }

    @Test("A single voucher that cannot fit the output limit is reported rather than submitted")
    func unfittableSingleVoucherThrows() async throws {
        let vouchers = [makeVoucher(exponent: 7, derivationIndex: 0)]

        await #expect(throws: CoinSelectionError.self) {
            try await makeSelector().selectCoins(SelectCoinsInput(
                amount: planks(Decimal(128)),
                coins: [],
                vouchers: vouchers,
                breakdownContext: testContext,
                limits: UnloadCallLimits(maxVouchersPerCall: 2, maxOutputsPerCall: 0)
            ))
        }
    }
}
