import CoreData
import Foundation
import Operation_iOS
import SubstrateSdk

// The stored forms of the Chat Extension v2 additions (07-LATER §4.4). The store keeps the features of a room and the
// meta of a message as one version byte, then the SCALE bytes of the wire value in wire field order. The version byte
// lets a later build change the stored form (02-DESIGN §20 item 1).

extension Chat {
    /// The version byte at the start of the stored features of a room and the stored meta of a message.
    enum AdditionStoredForm {
        /// The first shape (`v01`) of `ChatRoomFeatures` and `ChatMessageMeta`.
        static let version: UInt8 = 1
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

extension Chat.RoomUserStateUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) {
        entity.mutedUntil = state.storedMutedUntil
        entity.archived = state.archived

        // The chat list observes `CDChat` only. This change of the chat makes the list draw the new state.
        let key = #keyPath(CDChat.roomMetadata)
        entity.chat?.willChangeValue(forKey: key)
        entity.chat?.didChangeValue(forKey: key)
    }
}

// MARK: - Stored user state

/// The store keeps `mutedUntil` as a signed integer: 0 is not muted, `Int64.max` is muted with no end.
extension Chat.RoomUserState {
    init(entity: CDChatRoomMetadata) {
        self.init(mutedUntil: Self.mutedUntil(stored: entity.mutedUntil), archived: entity.archived)
    }

    var storedMutedUntil: Int64 {
        switch mutedUntil {
        case nil: 0
        case Self.mutedForever: Int64.max
        case let .some(until): Int64(clamping: until)
        }
    }

    static func mutedUntil(stored: Int64) -> UInt64? {
        switch stored {
        case ...0: nil
        case Int64.max: mutedForever
        default: UInt64(stored)
        }
    }
}

// MARK: - SCALE forms

// Each value has the SCALE layout of its wire type of `v01`. The field and variant order is a wire contract.

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

extension Chat.ThreadParent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        roomId = try String(scaleDecoder: scaleDecoder)
        messageId = try String(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try roomId.encode(scaleEncoder: scaleEncoder)
        try messageId.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RoomFeatures: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        preimageAttachments = try Bool(scaleDecoder: scaleDecoder)
        search = try Bool(scaleDecoder: scaleDecoder)
        calls = try ScaleOption<Chat.RoomCalls>(scaleDecoder: scaleDecoder).value
        threadOf = try ScaleOption<Chat.ThreadParent>(scaleDecoder: scaleDecoder).value
        readAloud = try Bool(scaleDecoder: scaleDecoder)
        sealedAttachments = scaleDecoder.remained > 0 ? try Bool(scaleDecoder: scaleDecoder) : false
        inputEvents = scaleDecoder.remained > 0 ? try Bool(scaleDecoder: scaleDecoder) : false
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try preimageAttachments.encode(scaleEncoder: scaleEncoder)
        try search.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: calls).encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: threadOf).encode(scaleEncoder: scaleEncoder)
        try readAloud.encode(scaleEncoder: scaleEncoder)
        try sealedAttachments.encode(scaleEncoder: scaleEncoder)
        try inputEvents.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.ThreadSummary: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        roomId = try String(scaleDecoder: scaleDecoder)
        replyCount = try UInt32(scaleDecoder: scaleDecoder)
        lastReplyAt = try ScaleOption<UInt64>(scaleDecoder: scaleDecoder).value
        participantIds = try [String](scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try roomId.encode(scaleEncoder: scaleEncoder)
        try replyCount.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: lastReplyAt).encode(scaleEncoder: scaleEncoder)
        try participantIds.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.BranchPosition: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        index = try UInt16(scaleDecoder: scaleDecoder)
        count = try UInt16(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try index.encode(scaleEncoder: scaleEncoder)
        try count.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.Reasoning: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        durationMs = try ScaleOption<UInt32>(scaleDecoder: scaleDecoder).value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try ScaleOption(value: durationMs).encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.MessageMeta: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        thread = try ScaleOption<Chat.ThreadSummary>(scaleDecoder: scaleDecoder).value
        branch = try ScaleOption<Chat.BranchPosition>(scaleDecoder: scaleDecoder).value
        reasoning = try ScaleOption<Chat.Reasoning>(scaleDecoder: scaleDecoder).value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try ScaleOption(value: thread).encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: branch).encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: reasoning).encode(scaleEncoder: scaleEncoder)
    }
}
