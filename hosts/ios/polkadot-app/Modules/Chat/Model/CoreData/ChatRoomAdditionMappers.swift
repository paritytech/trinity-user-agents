import CoreData
import Foundation
import Operation_iOS
import SubstrateSdk

// The stored forms of the Chat Extension v2 additions (07-LATER §4.4) and of the contact of a room. The store keeps
// each value as one version byte, then its SCALE bytes. The version byte lets a later build change the stored form
// (02-DESIGN §20 item 1).

extension Chat {
    /// The version byte at the start of a stored value.
    enum AdditionStoredForm {
        /// The second shape: the features hold only `calls` and `hop_attachments`.
        static let version: UInt8 = 2
    }
}

extension ScaleEncodable {
    /// The version byte, then the SCALE bytes of the value.
    func storedBytes() throws -> Data {
        let encoder = ScaleEncoder()
        try Chat.AdditionStoredForm.version.encode(scaleEncoder: encoder)
        try encode(scaleEncoder: encoder)
        return encoder.encode()
    }
}

extension ScaleDecodable {
    /// The value of stored bytes, or nil when the version is not known or the bytes are not a complete value.
    static func decodedIfStored(_ data: Data) -> Self? {
        guard data.first == Chat.AdditionStoredForm.version else {
            return nil
        }

        return decodedIfValid(Data(data.dropFirst()))
    }
}

extension Chat.RoomFeaturesUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) throws {
        entity.features = try features.storedBytes()
    }
}

// MARK: - SCALE forms

// The features have the SCALE layout of their wire type. The field and variant order is a wire contract.

extension Chat.RoomCalls: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        audio = try Bool(scaleDecoder: scaleDecoder)
        video = try Bool(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try audio.encode(scaleEncoder: scaleEncoder)
        try video.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RoomFeatures: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        calls = try ScaleOption<Chat.RoomCalls>(scaleDecoder: scaleDecoder).value
        hopAttachments = try Bool(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try ScaleOption(value: calls).encode(scaleEncoder: scaleEncoder)
        try hopAttachments.encode(scaleEncoder: scaleEncoder)
    }
}

/// The account is a `[u8; 32]`, which SCALE encodes as 32 bytes with no length prefix.
extension Chat.RoomContact: ScaleCodable {
    private static let accountLength = 32

    init(scaleDecoder: any ScaleDecoding) throws {
        account = try scaleDecoder.readAndConfirm(count: Self.accountLength)
        username = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
        guard let state = try State(rawValue: UInt8(scaleDecoder: scaleDecoder)) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }
        self.state = state
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        guard account.count == Self.accountLength else {
            throw ScaleCodingError.unexpectedDecodedValue
        }

        scaleEncoder.appendRaw(data: account)
        try ScaleOption(value: username).encode(scaleEncoder: scaleEncoder)
        try state.rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

/// The host form of the last message: its content has the stored form of a message of the native chat.
extension Chat.RoomPreviewMessage: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        messageId = try String(scaleDecoder: scaleDecoder)
        status = try Chat.LocalMessage.Status.stored(scaleDecoder)
        timestamp = try UInt64(scaleDecoder: scaleDecoder)
        content = try Chat.LocalMessage.Content(scaleDecoder: scaleDecoder)
        relatedMessages = try [Chat.RelatedLocalMessage](scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try messageId.encode(scaleEncoder: scaleEncoder)
        try UInt8(clamping: status.rawValue).encode(scaleEncoder: scaleEncoder)
        try timestamp.encode(scaleEncoder: scaleEncoder)
        try content.encode(scaleEncoder: scaleEncoder)
        try relatedMessages.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RelatedLocalMessage: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        messageId = try String(scaleDecoder: scaleDecoder)
        status = try Chat.LocalMessage.Status.stored(scaleDecoder)
        timestamp = try UInt64(scaleDecoder: scaleDecoder)
        content = try Chat.LocalMessage.Content(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try messageId.encode(scaleEncoder: scaleEncoder)
        try UInt8(clamping: status.rawValue).encode(scaleEncoder: scaleEncoder)
        try timestamp.encode(scaleEncoder: scaleEncoder)
        try content.encode(scaleEncoder: scaleEncoder)
    }
}

private extension Chat.LocalMessage.Status {
    static func stored(_ scaleDecoder: any ScaleDecoding) throws -> Self {
        guard let status = try Self(rawValue: Int16(UInt8(scaleDecoder: scaleDecoder))) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }
        return status
    }
}
