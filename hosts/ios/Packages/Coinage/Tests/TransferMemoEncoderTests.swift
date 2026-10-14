import Testing
import Foundation
import SubstrateSdk
@testable import Coinage

@Suite("TransferMemoEncoder Tests")
struct TransferMemoEncoderTests {
    private let encoder = SCALEMemoEncoder()

    // MARK: - Encode Tests

    @Test("Encode produces non-empty Data")
    func encodeProducesNonEmptyData() throws {
        // Given
        let entry = TransferCoinEntry(
            privateKey: Data(repeating: 0xAB, count: 32),
            exponent: 3,
            age: 42
        )

        // When
        let encoded = try encoder.encode(entry)

        // Then
        #expect(!encoded.isEmpty)
    }

    @Test("Encode roundtrip returns identical entry")
    func encodeRoundtripReturnsIdenticalEntry() throws {
        // Given
        let original = TransferCoinEntry(
            privateKey: Data(repeating: 0xCD, count: 32),
            exponent: -2,
            age: 100
        )

        // When
        let encoded = try encoder.encode(original)
        let decoder = try ScaleDecoder(data: encoded)
        let decoded = try TransferCoinEntry(scaleDecoder: decoder)

        // Then
        #expect(decoded == original)
    }

    @Test("Encode preserves privateKey field")
    func encodePreservesPrivateKey() throws {
        // Given
        let expectedKey = Data((0 ..< 32).map { UInt8($0) })
        let entry = TransferCoinEntry(
            privateKey: expectedKey,
            exponent: 0,
            age: 0
        )

        // When
        let encoded = try encoder.encode(entry)
        let decoder = try ScaleDecoder(data: encoded)
        let decoded = try TransferCoinEntry(scaleDecoder: decoder)

        // Then
        #expect(decoded.privateKey == expectedKey)
    }

    @Test("Encode preserves exponent field")
    func encodePreservesExponent() throws {
        // Given
        let expectedExponent: Int16 = -5
        let entry = TransferCoinEntry(
            privateKey: Data(repeating: 0, count: 32),
            exponent: expectedExponent,
            age: 0
        )

        // When
        let encoded = try encoder.encode(entry)
        let decoder = try ScaleDecoder(data: encoded)
        let decoded = try TransferCoinEntry(scaleDecoder: decoder)

        // Then
        #expect(decoded.exponent == expectedExponent)
    }

    @Test("Encode preserves age field")
    func encodePreservesAge() throws {
        // Given
        let expectedAge: Int32 = 999
        let entry = TransferCoinEntry(
            privateKey: Data(repeating: 0, count: 32),
            exponent: 0,
            age: expectedAge
        )

        // When
        let encoded = try encoder.encode(entry)
        let decoder = try ScaleDecoder(data: encoded)
        let decoded = try TransferCoinEntry(scaleDecoder: decoder)

        // Then
        #expect(decoded.age == expectedAge)
    }

    @Test("Different entries produce different encoded Data")
    func differentEntriesProduceDifferentData() throws {
        // Given
        let entry1 = TransferCoinEntry(
            privateKey: Data(repeating: 0x11, count: 32),
            exponent: 1,
            age: 10
        )
        let entry2 = TransferCoinEntry(
            privateKey: Data(repeating: 0x22, count: 32),
            exponent: 2,
            age: 20
        )

        // When
        let encoded1 = try encoder.encode(entry1)
        let encoded2 = try encoder.encode(entry2)

        // Then
        #expect(encoded1 != encoded2)
    }

    @Test("Entries with same values produce identical encoded Data")
    func sameEntriesProduceIdenticalData() throws {
        // Given
        let entry1 = TransferCoinEntry(
            privateKey: Data(repeating: 0xFF, count: 32),
            exponent: 7,
            age: 50
        )
        let entry2 = TransferCoinEntry(
            privateKey: Data(repeating: 0xFF, count: 32),
            exponent: 7,
            age: 50
        )

        // When
        let encoded1 = try encoder.encode(entry1)
        let encoded2 = try encoder.encode(entry2)

        // Then
        #expect(encoded1 == encoded2)
    }

    @Test("Encoded data has expected size for TransferCoinEntry")
    func encodedDataHasExpectedSize() throws {
        // Given - TransferCoinEntry: 32 bytes (privateKey) + 2 bytes (Int16 exponent) + 4 bytes (Int32 age) = 38 bytes
        let entry = TransferCoinEntry(
            privateKey: Data(repeating: 0, count: 32),
            exponent: 0,
            age: 0
        )

        // When
        let encoded = try encoder.encode(entry)

        // Then
        #expect(encoded.count == 38)
    }
}
