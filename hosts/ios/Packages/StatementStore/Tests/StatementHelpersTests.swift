import Foundation
import SubstrateSdk
import SubstrateSdkExt
import Testing
@testable import StatementStore

struct StatementHelpersTests {
    // MARK: - fromStatementFields

    @Test func fromStatementFieldsWithAllFieldsPopulated() throws {
        let topic1 = Data(repeating: 0x01, count: 32)
        let topic2 = Data(repeating: 0x02, count: 32)
        let channel = Data(repeating: 0x03, count: 32)
        let payload = Data([0x04, 0x05, 0x06])
        let expiry: UInt64 = 12_345
        let proof = StatementProof.sr25519(
            signature: Data(repeating: 0xAA, count: 64),
            signer: Data(repeating: 0xBB, count: 32)
        )

        let statement = try Statement.fromStatementFields(
            topics: [topic1, topic2],
            channel: channel,
            expiry: expiry,
            data: payload,
            proof: proof
        )

        let scaleEncodedPayload = try payload.scaleEncoded()

        #expect(statement.getProof() == proof)
        #expect(statement.getExpiry() == expiry)
        #expect(statement.getChannel() == channel)
        #expect(statement.getTopic1() == topic1)
        #expect(statement.getTopic2() == topic2)
        #expect(statement.getScaleEncodedPayload() == scaleEncodedPayload)
    }

    @Test func fromStatementFieldsWithNoOptionalFields() throws {
        let statement = try Statement.fromStatementFields(
            topics: [],
            channel: nil,
            expiry: nil,
            data: nil,
            proof: nil
        )

        #expect(statement.isEmpty)
    }

    @Test func fromStatementFieldsWithOnlyTopics() throws {
        let topic1 = Data(repeating: 0x01, count: 32)
        let topic2 = Data(repeating: 0x02, count: 32)
        let topic3 = Data(repeating: 0x03, count: 32)
        let topic4 = Data(repeating: 0x04, count: 32)

        let statement = try Statement.fromStatementFields(
            topics: [topic1, topic2, topic3, topic4],
            channel: nil,
            expiry: nil,
            data: nil,
            proof: nil
        )

        #expect(statement.count == 4)
        #expect(statement.getTopic1() == topic1)
        #expect(statement.getTopic2() == topic2)
        #expect(statement.getTopic3() == topic3)
        #expect(statement.getTopic4() == topic4)
    }

    @Test func fromStatementFieldsIgnoresTopicsBeyondFour() throws {
        let topics = (0 ..< 6).map { Data(repeating: UInt8($0), count: 32) }

        let statement = try Statement.fromStatementFields(
            topics: topics,
            channel: nil,
            expiry: nil,
            data: nil,
            proof: nil
        )

        #expect(statement.count == 4)
        #expect(statement.getTopic1() == topics[0])
        #expect(statement.getTopic2() == topics[1])
        #expect(statement.getTopic3() == topics[2])
        #expect(statement.getTopic4() == topics[3])
    }

    @Test func fromStatementFieldsResultIsSortedByScaleIndex() throws {
        let topic = Data(repeating: 0x01, count: 32)
        let channel = Data(repeating: 0x02, count: 32)
        let proof = StatementProof.sr25519(
            signature: Data(repeating: 0xAA, count: 64),
            signer: Data(repeating: 0xBB, count: 32)
        )

        let statement = try Statement.fromStatementFields(
            topics: [topic],
            channel: channel,
            expiry: 100,
            data: Data([0xFF]),
            proof: proof
        )

        let indices = statement.map(\.scaleIndex)
        #expect(indices == indices.sorted())
    }

    // MARK: - encodeForStore / SCALE round-trip

    @Test func encodeForStoreProducesValidScaleData() throws {
        let topic = Data(repeating: 0x01, count: 32)
        let proof = StatementProof.sr25519(
            signature: Data(repeating: 0xAA, count: 64),
            signer: Data(repeating: 0xBB, count: 32)
        )

        let statement = try Statement.fromStatementFields(
            topics: [topic],
            channel: nil,
            expiry: nil,
            data: nil,
            proof: proof
        )

        let encoded = try statement.encodeForStore()
        #expect(!encoded.isEmpty)
    }

    @Test func encodeDecodeRoundTripPreservesFields() throws {
        let topic1 = Data(repeating: 0x01, count: 32)
        let topic2 = Data(repeating: 0x02, count: 32)
        let channel = Data(repeating: 0x03, count: 32)
        let expiry: UInt64 = 9_999
        let payload = Data([0x10, 0x20, 0x30])
        let proof = StatementProof.sr25519(
            signature: Data(repeating: 0xAA, count: 64),
            signer: Data(repeating: 0xBB, count: 32)
        )

        let original = try Statement.fromStatementFields(
            topics: [topic1, topic2],
            channel: channel,
            expiry: expiry,
            data: payload,
            proof: proof
        )

        let encoded = try original.encodeForStore()
        let decoded = try Statement.fromScaleEncoded(encoded)

        let scaleEncodedPayload = try payload.scaleEncoded()

        #expect(decoded.getProof() == proof)
        #expect(decoded.getExpiry() == expiry)
        #expect(decoded.getChannel() == channel)
        #expect(decoded.getTopic1() == topic1)
        #expect(decoded.getTopic2() == topic2)
        #expect(decoded.getScaleEncodedPayload() == scaleEncodedPayload)
    }

    @Test func encodeDecodeRoundTripWithMinimalFields() throws {
        let proof = StatementProof.sr25519(
            signature: Data(repeating: 0x11, count: 64),
            signer: Data(repeating: 0x22, count: 32)
        )

        let original = try Statement.fromStatementFields(
            topics: [],
            channel: nil,
            expiry: nil,
            data: nil,
            proof: proof
        )

        let encoded = try original.encodeForStore()
        let decoded = try Statement.fromScaleEncoded(encoded)

        #expect(decoded.getProof() == proof)
        #expect(decoded.getExpiry() == nil)
        #expect(decoded.getChannel() == nil)
        #expect(decoded.getTopic1() == nil)
    }

    // MARK: - StatementSubmitParametersBuilding conformance

    @Test func buildReturnsNonEmptyEncodedStatement() throws {
        let proof = StatementProof.sr25519(
            signature: Data(repeating: 0xAA, count: 64),
            signer: Data(repeating: 0xBB, count: 32)
        )

        let statement = try Statement.fromStatementFields(
            topics: [Data(repeating: 0x01, count: 32)],
            channel: nil,
            expiry: nil,
            data: nil,
            proof: proof
        )

        let params = try statement.build()
        #expect(!params.encodedStatement.isEmpty)
    }

    @Test func buildProducesSameDataAsEncodeForStore() throws {
        let topic = Data(repeating: 0x01, count: 32)
        let proof = StatementProof.sr25519(
            signature: Data(repeating: 0xCC, count: 64),
            signer: Data(repeating: 0xDD, count: 32)
        )

        let statement = try Statement.fromStatementFields(
            topics: [topic],
            channel: Data(repeating: 0xEE, count: 32),
            expiry: 42,
            data: nil,
            proof: proof
        )

        let encoded = try statement.encodeForStore()
        let params = try statement.build()

        #expect(params.encodedStatement == encoded)
    }
}
