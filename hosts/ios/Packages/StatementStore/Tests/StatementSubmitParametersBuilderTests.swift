import Foundation
import SubstrateSdk
import SubstrateSdkExt
import Testing
@testable import StatementStore

struct StatementSubmitParametersBuilderTests {
    // MARK: - Builder with all field types

    @Test func builderWithAllFieldTypesProducesNonEmptyResult() throws {
        let signer = MockStatementStoreSigning()

        let params = try StatementSubmitParametersBuilder(signer: signer, logger: nil)
            .addChannel(Data(repeating: 0x01, count: 32))
            .addTopic1(Data(repeating: 0x02, count: 32))
            .addTopic2(Data(repeating: 0x03, count: 32))
            .addTopic3(Data(repeating: 0x04, count: 32))
            .addTopic4(Data(repeating: 0x05, count: 32))
            .addExpiry(12_345)
            .addScaleEncodedPayload(Data([0xAA, 0xBB]))
            .build()

        #expect(!params.encodedStatement.isEmpty)
    }

    // MARK: - Signer receives correct proof data

    @Test func buildSignsProofDataFromUnsignedFieldsOnly() throws {
        let signer = MockStatementStoreSigning()

        let topic = Data(repeating: 0x01, count: 32)
        let channel = Data(repeating: 0x02, count: 32)
        let expiry: UInt64 = 42

        _ = try StatementSubmitParametersBuilder(signer: signer, logger: nil)
            .addChannel(channel)
            .addTopic1(topic)
            .addExpiry(expiry)
            .build()

        #expect(signer.signedDataEntries.count == 1)

        // Reconstruct expected proof data from unsigned fields sorted by index
        let unsignedFields: Statement = [
            .channel(channel),
            .topic1(topic),
            .expiry(expiry)
        ]
        let expectedProofData = try unsignedFields.deriveProofData()

        #expect(signer.signedDataEntries[0] == expectedProofData)
    }

    // MARK: - Result decodes back to valid statement

    @Test func buildResultDecodesToStatementWithProof() throws {
        let signer = MockStatementStoreSigning()

        let topic = Data(repeating: 0x01, count: 32)

        let params = try StatementSubmitParametersBuilder(signer: signer, logger: nil)
            .addTopic1(topic)
            .addExpiry(100)
            .build()

        let decoded = try Statement.fromScaleEncoded(params.encodedStatement)

        #expect(decoded.getProof() != nil)
        #expect(decoded.getTopic1() == topic)
        #expect(decoded.getExpiry() == 100)
    }

    @Test func buildResultContainsSignerAccountInProof() throws {
        let accountId = Data(repeating: 0xCC, count: 32)
        let signer = MockStatementStoreSigning(accountId: accountId)

        let params = try StatementSubmitParametersBuilder(signer: signer, logger: nil)
            .addTopic1(Data(repeating: 0x01, count: 32))
            .build()

        let decoded = try Statement.fromScaleEncoded(params.encodedStatement)
        let proof = decoded.getProof()

        #expect(proof == .sr25519(
            signature: Data(repeating: 0xBB, count: StatementProof.signatureSize),
            signer: accountId
        ))
    }
}
