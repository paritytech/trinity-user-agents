import Foundation
import MessageExchangeKit
import SubstrateSdk
import SubstrateSdkExt

protocol ChatPushMessageDecoding {
    func decodeMessage(
        _ message: String,
        for contact: Chat.Contact
    ) throws -> Chat.NotificationPayload
}

protocol ChatPushMessageEncoding {
    func encodeMessage(
        _ payload: Chat.NotificationPayload,
        for contact: Chat.Contact
    ) throws -> String
}

typealias ChatPushMessageCoding = ChatPushMessageDecoding & ChatPushMessageEncoding

final class ChatPushMessageCoder {
    private let encryptionManager: MessageExchangeEncryptionManaging

    init(encryptionManager: MessageExchangeEncryptionManaging) {
        self.encryptionManager = encryptionManager
    }
}

extension ChatPushMessageCoder: ChatPushMessageCoding {
    func decodeMessage(
        _ message: String,
        for contact: Chat.Contact
    ) throws -> Chat.NotificationPayload {
        let encryptedData = try message.fromHex()
        let encryptor = try encryptionManager
            .makeEncryptorFactory(ownEncryptionKeyId: contact.ownKeyId.encryptionKeyId)
            .makeEncryptor(remotePublicKey: contact.publicKey)

        let decryptedData = try encryptor.decrypt(encryptedData)

        return try Chat.NotificationPayload.fromScaleEncoded(decryptedData)
    }

    func encodeMessage(
        _ payload: Chat.NotificationPayload,
        for contact: Chat.Contact
    ) throws -> String {
        let scaleData = try payload.scaleEncoded()
        let encryptor = try encryptionManager
            .makeEncryptorFactory(ownEncryptionKeyId: contact.ownKeyId.encryptionKeyId)
            .makeEncryptor(remotePublicKey: contact.publicKey)

        let encryptedData = try encryptor.encrypt(scaleData)
        return encryptedData.toHex()
    }
}
