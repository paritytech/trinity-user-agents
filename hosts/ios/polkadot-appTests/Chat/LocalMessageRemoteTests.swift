@testable import polkadot_app
import XCTest

final class LocalMessageRemoteTests: XCTestCase {
    func testDeviceUpdateMessagesCanSendToRemote() {
        let statementAccountId = Data(repeating: 1, count: 32)
        let encryptionPublicKey = Data(repeating: 2, count: 32)

        let deviceAdded = Chat.LocalMessage.Content.deviceAdded(.init(
            statementAccountId: statementAccountId,
            encryptionPublicKey: encryptionPublicKey
        ))
        let deviceRemoved = Chat.LocalMessage.Content.deviceRemoved(.init(
            statementAccountId: statementAccountId
        ))

        XCTAssertTrue(deviceAdded.canSendToRemote())
        XCTAssertTrue(deviceRemoved.canSendToRemote())
    }

    func testDeviceUpdateMessagesConvertToRemoteContent() {
        let statementAccountId = Data(repeating: 1, count: 32)
        let encryptionPublicKey = Data(repeating: 2, count: 32)

        let deviceAdded = Chat.LocalMessage.Content.deviceAdded(.init(
            statementAccountId: statementAccountId,
            encryptionPublicKey: encryptionPublicKey
        ))
        let deviceRemoved = Chat.LocalMessage.Content.deviceRemoved(.init(
            statementAccountId: statementAccountId
        ))

        XCTAssertEqual(
            deviceAdded.toRemote(),
            .v1(.init(content: .deviceAdded(.init(
                statementAccountId: statementAccountId,
                encryptionPublicKey: encryptionPublicKey
            ))))
        )
        XCTAssertEqual(
            deviceRemoved.toRemote(),
            .v1(.init(content: .deviceRemoved(.init(
                statementAccountId: statementAccountId
            ))))
        )
    }
}
