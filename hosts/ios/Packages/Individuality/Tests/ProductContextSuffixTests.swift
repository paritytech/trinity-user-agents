import Foundation
import SubstrateSdk
import Testing
@testable import Individuality

struct ProductContextSuffixTests {
    private let networkSuffix = Data("paseo".utf8)

    @Test func statementStoreSlotContextMatchesPalletVector() throws {
        // Vectors pin the family-to-layout binding directly.
        let context = try ProductContextSuffix
            .statementStoreSlot(period: 100, seq: 3)
            .context(networkSuffix: networkSuffix)
        let expected = try Data(hexString: "b6c21225dcf4c2aeeca32b6db1fc93b6942ca0e8ff5c3cb1b2c5d8f0b4647ee3")
        #expect(context == expected)
    }

    @Test func longTermStorageContextMatchesPalletVector() throws {
        let context = try ProductContextSuffix
            .longTermStorage(period: 100, counter: 3)
            .context(networkSuffix: networkSuffix)
        let expected = try Data(hexString: "1b3fbe4dd813ea1e349878c9228c6823db8345207690ca4df656acb7fee81bd1")
        #expect(context == expected)
    }

    @Test func pgasClaimContextMatchesPalletVector() throws {
        let context = try ProductContextSuffix
            .pgasClaim(day: 100, slot: 3)
            .context(networkSuffix: networkSuffix)
        let expected = try Data(hexString: "e47ba2c7eae3b97beabaeef8df599afd53e44ba9c2b851cd80850d3ed95a685b")
        #expect(context == expected)
    }

    @Test func notificationSlotContextMatchesPalletVector() throws {
        let context = try ProductContextSuffix
            .notificationSlot(period: 100, seq: 3)
            .context(networkSuffix: networkSuffix)
        let expected = try Data(hexString: "be3852b67f633d0da121aa3163dfe004f5056e71bc6a1d005793edb55287be24")
        #expect(context == expected)
    }

    @Test func zeroValuesProduceDistinctContext() throws {
        let context = try ProductContextSuffix
            .statementStoreSlot(period: 0, seq: 0)
            .context(networkSuffix: networkSuffix)
        let expected = try Data(hexString: "deee1c90cf0d31093d318ac6629b4c4ab08650d4a4164511cc2496205f20f067")
        #expect(context == expected)
    }

    @Test func suffixLayoutIsPinned() throws {
        // Notification family 1, period 100, seq 3
        let notificationSuffix = ProductContextSuffix.notificationSlot(period: 100, seq: 3).bytes
        let notificationExpected =
            try Data(hexString: "7379732f01000000640000000300000000000000000000000000000000000000")
        #expect(notificationSuffix == notificationExpected)

        // SSS family 2, period 100, seq 3
        let sssSuffix = ProductContextSuffix.statementStoreSlot(period: 100, seq: 3).bytes
        let sssExpected =
            try Data(hexString: "7379732f02000000640000000300000000000000000000000000000000000000")
        #expect(sssSuffix == sssExpected)

        // LTS family 3, period 100, counter 3
        let ltsSuffix = ProductContextSuffix.longTermStorage(period: 100, counter: 3).bytes
        let ltsExpected =
            try Data(hexString: "7379732f03000000640000000300000000000000000000000000000000000000")
        #expect(ltsSuffix == ltsExpected)

        // PGAS family 4, day 100, slot 3
        let pgasSuffix = ProductContextSuffix.pgasClaim(day: 100, slot: 3).bytes
        let pgasExpected =
            try Data(hexString: "7379732f04000000640000000300000000000000000000000000000000000000")
        #expect(pgasSuffix == pgasExpected)

        // SSS family 2, period 0, seq 0
        let zeroSuffix = ProductContextSuffix.statementStoreSlot(period: 0, seq: 0).bytes
        let zeroExpected =
            try Data(hexString: "7379732f02000000000000000000000000000000000000000000000000000000")
        #expect(zeroSuffix == zeroExpected)
    }

    @Test func differentNetworkSuffixChangesContext() throws {
        let suffix = ProductContextSuffix.statementStoreSlot(period: 100, seq: 3)

        let paseoContext = try suffix.context(networkSuffix: Data("paseo".utf8))
        let dotContext = try suffix.context(networkSuffix: Data("dot".utf8))

        #expect(paseoContext != dotContext)
    }
}
