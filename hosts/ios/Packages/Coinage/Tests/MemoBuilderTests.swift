import Testing
import Foundation
import BigInt
@testable import Coinage

@Suite("MemoBuilder Tests")
struct MemoBuilderTests {
    private let deriver = MockCoinKeyDeriver()

    private lazy var builder: MemoBuilding = MemoBuilder(privateKeyDeriver: deriver)

    // MARK: - Mock Private Key Deriver

    private final class MockCoinKeyDeriver: CoinKeyDeriving, @unchecked Sendable {
        private let mutex = NSLock()

        private var storedDerivedKeys: [CoinageKeyIndex: Data] = [:]
        private var storedError: Error?

        var derivedKeys: [CoinageKeyIndex: Data] {
            get { mutex.withLock { storedDerivedKeys } }
            set { mutex.withLock { storedDerivedKeys = newValue } }
        }

        var shouldThrow: Error? {
            get { mutex.withLock { storedError } }
            set { mutex.withLock { storedError = newValue } }
        }

        func derivePublicKey(index _: CoinageKeyIndex) throws -> PublicKey {
            Data(repeating: 0, count: 32)
        }

        func derivePrivateKey(index: CoinageKeyIndex) throws -> PrivateKey {
            if let error = shouldThrow {
                throw error
            }
            if let key = derivedKeys[index] {
                return key
            }
            // Default: generate deterministic key based on derivation index
            return Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32)
        }
    }

    // MARK: - Test Helpers

    /// Standard context: 1 unit = 10^16 planks (1 cent), precision 18
    private let context = DenominationBreakdownContext(
        unit: BigUInt(10).power(16),
        precision: 18,
        maxExponent: 7,
        minExponent: 0
    )

    private func makeEntry(
        exponent: Int16,
        derivationIndex: CoinageKeyIndex
    ) -> PlannedMemoEntry {
        PlannedMemoEntry(coinDerivationIndex: derivationIndex, valueExponent: exponent)
    }

    // MARK: - buildMemo Tests

    @Test("Builds memo with a single entry containing raw private key")
    mutating func buildMemoSingleEntry() throws {
        // Given
        let expectedKey = Data(repeating: 0xAB, count: 32)
        deriver.derivedKeys[42] = expectedKey

        let entries = [makeEntry(exponent: 0, derivationIndex: 42)]

        // When
        let memo = try builder.buildMemo(from: entries, breakdownContext: context)

        // Then
        #expect(memo.entries.count == 1)
        #expect(memo.entries[0] == expectedKey)
    }

    @Test("Builds memo with multiple entries containing raw private keys")
    mutating func buildMemoMultipleEntries() throws {
        // Given
        deriver.derivedKeys[1] = Data(repeating: 0x11, count: 32)
        deriver.derivedKeys[2] = Data(repeating: 0x22, count: 32)
        deriver.derivedKeys[3] = Data(repeating: 0x33, count: 32)

        let entries = [
            makeEntry(exponent: 0, derivationIndex: 1),
            makeEntry(exponent: 2, derivationIndex: 2),
            makeEntry(exponent: 5, derivationIndex: 3),
        ]

        // When
        let memo = try builder.buildMemo(from: entries, breakdownContext: context)

        // Then
        #expect(memo.entries.count == 3)
        #expect(memo.entries[0] == Data(repeating: 0x11, count: 32))
        #expect(memo.entries[1] == Data(repeating: 0x22, count: 32))
        #expect(memo.entries[2] == Data(repeating: 0x33, count: 32))
    }

    @Test("Preserves entry order in memo")
    mutating func buildMemoPreservesEntryOrder() throws {
        // Given
        deriver.derivedKeys[100] = Data(repeating: 0xAA, count: 32)
        deriver.derivedKeys[50] = Data(repeating: 0xBB, count: 32)
        deriver.derivedKeys[200] = Data(repeating: 0xCC, count: 32)

        let entries = [
            makeEntry(exponent: 3, derivationIndex: 100),
            makeEntry(exponent: 0, derivationIndex: 50),
            makeEntry(exponent: 5, derivationIndex: 200),
        ]

        // When
        let memo = try builder.buildMemo(from: entries, breakdownContext: context)

        // Then - order should match input order
        #expect(memo.entries[0] == Data(repeating: 0xAA, count: 32))
        #expect(memo.entries[1] == Data(repeating: 0xBB, count: 32))
        #expect(memo.entries[2] == Data(repeating: 0xCC, count: 32))
    }

    @Test("Calculates totalValue in planks from breakdown context")
    mutating func buildMemoCalculatesTotalValue() throws {
        // Given
        let entries = [
            makeEntry(exponent: 0, derivationIndex: 1),
            makeEntry(exponent: 1, derivationIndex: 2),
            makeEntry(exponent: 2, derivationIndex: 3),
        ]

        // When
        let memo = try builder.buildMemo(from: entries, breakdownContext: context)

        // Then - totalValue should be sum of valueInPlanks
        let expectedTotal = context.valueInPlanks(for: 0)
            + context.valueInPlanks(for: 1)
            + context.valueInPlanks(for: 2)
        #expect(memo.totalValue == expectedTotal)
    }

    // MARK: - Raw Private Key Tests

    @Test("Entries are raw 32-byte private keys")
    mutating func buildMemoEntriesAreRawPrivateKeys() throws {
        // Given
        let expectedKey = Data(repeating: 0xCD, count: 32)
        deriver.derivedKeys[1] = expectedKey
        let entries = [makeEntry(exponent: 5, derivationIndex: 1)]

        // When
        let memo = try builder.buildMemo(from: entries, breakdownContext: context)

        // Then - entry is raw 32-byte private key (no encoding)
        #expect(memo.entries[0].count == 32)
        #expect(memo.entries[0] == expectedKey)
    }

    @Test("Entry count matches input PlannedMemoEntry count")
    mutating func buildMemoEntryCountMatchesInput() throws {
        // Given
        let entries = [
            makeEntry(exponent: 0, derivationIndex: 1),
            makeEntry(exponent: 1, derivationIndex: 2),
            makeEntry(exponent: 2, derivationIndex: 3),
            makeEntry(exponent: 3, derivationIndex: 4),
            makeEntry(exponent: 4, derivationIndex: 5),
        ]

        // When
        let memo = try builder.buildMemo(from: entries, breakdownContext: context)

        // Then
        #expect(memo.entries.count == entries.count)
    }

    // MARK: - Error Cases

    @Test("Throws emptyCoins error for empty entries")
    mutating func buildMemoEmptyEntriesThrows() {
        // When/Then
        #expect(throws: MemoBuilderError.self) {
            try builder.buildMemo(from: [], breakdownContext: context)
        }
    }

    @Test("Wraps key derivation failure in keyDerivationFailed error")
    mutating func buildMemoKeyDerivationFailurePropagates() {
        // Given
        let testError = NSError(domain: "test", code: 42, userInfo: nil)
        deriver.shouldThrow = testError

        let entries = [makeEntry(exponent: 0, derivationIndex: 1)]

        // When/Then
        do {
            _ = try builder.buildMemo(from: entries, breakdownContext: context)
            Issue.record("Expected MemoBuilderError.keyDerivationFailed to be thrown")
        } catch let error as MemoBuilderError {
            guard case let .keyDerivationFailed(underlying) = error else {
                Issue.record("Expected keyDerivationFailed, got \(error)")
                return
            }
            #expect((underlying as NSError).code == 42)
        } catch {
            Issue.record("Unexpected error type: \(error)")
        }
    }

    @Test("Key derivation failure stops processing remaining entries")
    mutating func buildMemoKeyDerivationFailureStopsProcessing() {
        // Given - deriver fails on second coin
        deriver.derivedKeys[1] = Data(repeating: 0x11, count: 32)
        // No key for index 999

        let entries = [
            makeEntry(exponent: 0, derivationIndex: 1),
            makeEntry(exponent: 0, derivationIndex: 999), // Will fail
        ]

        // Set error after first coin would succeed
        deriver.shouldThrow = NSError(domain: "test", code: 1, userInfo: nil)

        // When/Then
        #expect(throws: MemoBuilderError.self) {
            try builder.buildMemo(from: entries, breakdownContext: context)
        }
    }
}
