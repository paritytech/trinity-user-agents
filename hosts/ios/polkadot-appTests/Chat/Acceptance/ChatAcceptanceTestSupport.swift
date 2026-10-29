import Foundation
import SubstrateSdk
@testable import polkadot_app

/// An acceptance toward a fixed peer, as the outgoing pipeline sees it.
struct ChatAcceptanceFixture {
    let now = Date(timeIntervalSince1970: 1_700_000_000)
    let requestId = "request"
    let identityKeyId = "//wallet//main"
    let peerAccountId = Data(repeating: 9, count: 32)

    func makeMessage(id: String = "acceptance", status: Chat.LocalMessage.Status = .outgoing(.new)) -> Chat
        .LocalMessage {
        Chat.LocalMessage(
            messageId: id,
            chatId: .person(peerAccountId),
            origin: .user,
            creationSource: .localDevice,
            status: status,
            timestamp: now.toChatTimestamp(),
            content: .multiChatAccepted(.init(
                requestId: requestId,
                device: Chat.PeerDevice(
                    statementAccountId: Data(repeating: 2, count: 32),
                    encryptionPublicKey: Data(repeating: 3, count: 32)
                )
            )),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }

    func makeInput(id: String = "acceptance") -> OutgoingChatAcceptanceInput {
        OutgoingChatAcceptanceInput(message: makeMessage(id: id), requestId: requestId, contact: makeContact())
    }

    func makeContact() -> Chat.Contact {
        Chat.Contact(
            accountId: peerAccountId,
            username: "peer",
            publicKey: Data(repeating: 0x02, count: 32),
            pin: nil,
            pushId: nil,
            pushToken: Data(repeating: 0x03, count: 32),
            voipPushToken: nil,
            peerPlatform: nil,
            lastOwnToken: nil,
            voipLastOwnToken: nil,
            chatRequest: nil,
            ownKeyId: .init(signKeyId: identityKeyId, encryptionKeyId: ""),
            imageData: nil,
            isBlocked: false,
            devices: [],
            pendingDevicesFanOut: false
        )
    }
}
