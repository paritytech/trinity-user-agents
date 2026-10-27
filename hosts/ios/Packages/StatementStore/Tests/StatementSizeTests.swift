import Foundation
import SubstrateSdk
import Testing
@testable import StatementStore

struct StatementSizeTests {
    @Test(arguments: [1, 3])
    func overheadMatchesEncodedStatement(topicCount: Int) throws {
        let payload = try Data(repeating: 0xAA, count: 300).scaleEncoded()
        let topic = Data(repeating: 0x01, count: StatementFieldConstants.fixedFieldSize)

        let builder = StatementSubmitParametersBuilder(signer: MockStatementStoreSigning(), logger: nil)
            .addChannel(Data(repeating: 0xCC, count: StatementFieldConstants.fixedFieldSize))
            .addExpiry(12_345)
            .addScaleEncodedPayload(payload)
            .addTopic1(topic)

        if topicCount == 3 {
            builder.addTopic2(topic).addTopic3(topic)
        }

        let encoded = try builder.build().encodedStatement

        #expect(encoded.count == payload.count + StatementSize.overhead(topicCount: topicCount))
    }
}
