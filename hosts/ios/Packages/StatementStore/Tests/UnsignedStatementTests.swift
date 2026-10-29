import Foundation
import Testing
@testable import StatementStore

struct UnsignedStatementTests {
    @Test func signingMatchesUpFrontBuilder() throws {
        let signer = MockStatementStoreSigning(accountId: Data(repeating: 0xCC, count: 32))
        let topic = Data(repeating: 0x01, count: 32)
        let channel = Data(repeating: 0x02, count: 32)
        let payload = Data([0xAA, 0xBB])

        let deferred = try UnsignedStatement()
            .addTopic1(topic)
            .addChannel(channel)
            .addExpiry(42)
            .addScaleEncodedPayload(payload)
            .signed(by: signer)
            .build()
        let upFront = try StatementSubmitParametersBuilder(signer: signer, logger: nil)
            .addTopic1(topic)
            .addChannel(channel)
            .addExpiry(42)
            .addScaleEncodedPayload(payload)
            .build()

        #expect(deferred.encodedStatement == upFront.encodedStatement)
    }

    @Test func sameStatementCanBeSignedByDifferentAccounts() throws {
        let statement = UnsignedStatement().addTopic1(Data(repeating: 0x01, count: 32))
        let first = Data(repeating: 0xC1, count: 32)
        let second = Data(repeating: 0xC2, count: 32)

        let signers = try [first, second].map { accountId in
            let params = try statement.signed(by: MockStatementStoreSigning(accountId: accountId)).build()
            return try Statement.fromScaleEncoded(params.encodedStatement).getSenderAccountId()
        }

        #expect(signers == [first, second])
    }
}

struct UnsignedStatementSizeTests {
    @Test func signedSizeMatchesTheSignedEncoding() throws {
        let statement = UnsignedStatement()
            .addTopic1(Data(repeating: 0x01, count: 32))
            .addChannel(Data(repeating: 0x02, count: 32))
            .addExpiry(42)
            .addScaleEncodedPayload(Data(repeating: 0xAA, count: 100))

        let signed = try statement.signed(by: MockStatementStoreSigning()).build()

        #expect(try statement.signedSize() == signed.encodedStatement.count)
    }
}
