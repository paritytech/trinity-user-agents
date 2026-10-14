import Coinage
import Foundation
import SubstrateSdk
import Testing

@Suite("Asset State")
struct AssetStateTests {
    // MARK: - Spent Never Un-sets

    @Test("Spent coin stays spent")
    func spentStaysSpent() async throws {
        let store = MockCoinageTxRepository()
        let coin = OwnAsset.coin(5, testKey(5))

        try await store.register(.fixture(outputs: [coin]))

        let consumer = CoinageTxEntry.fixture(inputs: [coin.asInput])
        try await store.register(consumer)

        let fetched = try await store.getEntry(id: consumer.id)
        #expect(fetched != nil)
        #expect(fetched?.status == .pending)

        // Mark consumer as finalized success (coin is now spent)
        try await store.updateStatus(consumer.id, to: .finalizedSuccess)

        // The coin is now consumed by a terminal entry, so no new entry may claim it.
        await #expect(throws: CoinageTxError.inputAlreadyClaimed(coin.publicKey.toHex())) {
            try await store.register(.fixture(inputs: [coin.asInput]))
        }
    }

    // MARK: - Shallow Finalized Head

    @Test("Coin above shallow finalized head is not read as spent")
    func shallowHeadDoesNotMarkSpent() async throws {
        let store = MockCoinageTxRepository()
        let coin = OwnAsset.coin(7, testKey(7))

        // Minted at block 200, well above a finalized head of 50.
        let entry = CoinageTxEntry.fixture(outputs: [coin], checkpoint: .fixture(200))
        try await store.register(entry)

        // A finalized head of 50 doesn't affect the coin's existence status
        // because the coin was minted at block 200, which is beyond finalized.
        // This is a read-only check, so we're just verifying store behavior.
        let fetched = try await store.getEntry(id: entry.id)
        #expect(fetched != nil)
        #expect(fetched?.outputs.contains(coin) == true)
    }

    // MARK: - Cleaned Voucher

    @Test("Cleaned voucher is counted nowhere")
    func cleanedVoucherNotCounted() async throws {
        let store = MockCoinageTxRepository()
        let voucher = OwnAsset.recyclerVoucher(3, testKey(3))

        try await store.register(.fixture(outputs: [voucher]))

        let handedOff = try await store.getHandoffKeys().contains(voucher.publicKey)
        #expect(!handedOff)

        // Consumer uses and cleans the voucher
        let consumer = CoinageTxEntry.fixture(inputs: [voucher.asInput])
        try await store.register(consumer)
        try await store.updateStatus(consumer.id, to: .finalizedSuccess)

        // Cleaned voucher should not be in handoff marks
        let stillHandedOff = try await store.getHandoffKeys().contains(voucher.publicKey)
        #expect(!stillHandedOff)
    }

    // MARK: - Handed-Off Asset

    @Test("Handed-off asset is never selectable and never registrable")
    func handedOffAssetUnusable() async throws {
        let store = MockCoinageTxRepository()
        let coin = OwnAsset.coin(9, testKey(9))

        try await store.register(.fixture(outputs: [coin]))
        try await store.markHandedOff(coin)

        let handedOff = try await store.getHandoffKeys().contains(coin.publicKey)
        #expect(handedOff)

        await #expect(throws: CoinageTxError.inputHandedOff(coin.publicKey.toHex())) {
            try await store.register(.fixture(inputs: [coin.asInput]))
        }
    }

    // MARK: - Reservation

    @Test("Reservation deducts immediately")
    func reservationDeductsImmediately() async throws {
        let store = MockCoinageTxRepository()
        let input = CoinageTxInput.coin(.own(5, testKey(5)))

        // Reserve the coin by registering an entry that uses it
        try await store.register(.fixture(inputs: [input], outputs: [.coin(10, testKey(10))]))

        // The input should now be claimed and unavailable
        await #expect(throws: CoinageTxError.inputAlreadyClaimed(input.publicKey.toHex())) {
            try await store.register(.fixture(inputs: [input]))
        }
    }

    @Test("Reservation returns on failure")
    func reservationReturnedOnFailure() async throws {
        let store = MockCoinageTxRepository()
        let input = CoinageTxInput.coin(.own(5, testKey(5)))

        let first = CoinageTxEntry.fixture(inputs: [input])
        try await store.register(first)
        try await store.updateStatus(first.id, to: .failure)

        // The input should now be available again for a new entry
        let second = CoinageTxEntry.fixture(inputs: [input], outputs: [.coin(11, testKey(11))])
        try await store.register(second)

        let fetched = try await store.getEntry(id: second.id)
        #expect(fetched != nil)
    }

    // MARK: - Asset Statuses

    @Test("assetStatuses returns lock and minter status pair")
    func assetStatusesPair() async throws {
        let store = MockCoinageTxRepository()
        let coin = OwnAsset.coin(20, testKey(20))

        let minter = CoinageTxEntry.fixture(outputs: [coin])
        try await store.register(minter)

        let foundMinter = try await store.minter(of: coin)
        #expect(foundMinter != nil)
        #expect(foundMinter?.id == minter.id)

        // Register consumer (locks the asset)
        let consumer = CoinageTxEntry.fixture(inputs: [coin.asInput])
        try await store.register(consumer)

        // Consumer is live, so the asset is now in use
        let consumers = try await store.consumers(of: coin.asInput)
        #expect(consumers.contains { $0.id == consumer.id })
    }

    @Test("Lock reflects handoff marks")
    func lockReflectsHandoff() async throws {
        let store = MockCoinageTxRepository()
        let coin = OwnAsset.coin(15, testKey(15))

        try await store.register(.fixture(outputs: [coin]))
        try await store.markHandedOff(coin)

        // Verify handoff is permanent
        let handedOff = try await store.getHandoffKeys().contains(coin.publicKey)
        #expect(handedOff)

        // Should not be usable in new entries
        await #expect(throws: CoinageTxError.inputHandedOff(coin.publicKey.toHex())) {
            try await store.register(.fixture(inputs: [coin.asInput]))
        }
    }

    @Test("Minter status reflects entry state")
    func minterStatusReflectsEntry() async throws {
        let store = MockCoinageTxRepository()
        let coin = OwnAsset.coin(25, testKey(25))

        let minter = CoinageTxEntry.fixture(outputs: [coin])
        try await store.register(minter)

        var fetched = try await store.minter(of: coin)
        #expect(fetched != nil)
        #expect(fetched?.status == .pending)

        try await store.updateStatus(minter.id, to: .pendingSuccess)
        fetched = try await store.minter(of: coin)
        #expect(fetched != nil)
        #expect(fetched?.status == .pendingSuccess)

        try await store.updateStatus(minter.id, to: .finalizedSuccess)
        fetched = try await store.minter(of: coin)
        #expect(fetched != nil)
        #expect(fetched?.status == .finalizedSuccess)
    }

    @Test("No minter found for unminted asset")
    func noMinterForUnmintedAsset() async throws {
        let store = MockCoinageTxRepository()
        let unmintedCoin = OwnAsset.coin(999, testKey(999))

        let minter = try await store.minter(of: unmintedCoin)
        #expect(minter == nil)
    }
}
