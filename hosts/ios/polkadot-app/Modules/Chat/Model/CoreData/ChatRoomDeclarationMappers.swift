import CoreData
import Foundation
import Operation_iOS
import SubstrateSdk

/// A partial update of the declarations of one room. Each update writes only its own attributes.
protocol ChatRoomDeclarationUpdating: Identifiable {
    var chatId: Chat.Id { get }

    func apply(to entity: CDChatRoomMetadata) throws
}

extension ChatRoomDeclarationUpdating {
    var identifier: String {
        chatId.rawRepresentation
    }
}

/// Writes one partial update into the room of an existing chat, and leaves every other attribute alone.
final class ChatRoomDeclarationUpdateMapper<Update: ChatRoomDeclarationUpdating> {
    var entityIdentifierFieldName: String {
        #keyPath(CDChat.identifier)
    }

    typealias DataProviderModel = Update
    typealias CoreDataEntity = CDChat
}

extension ChatRoomDeclarationUpdateMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case missingChat
    }

    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        guard entity.identifier != nil else {
            throw MappingError.missingChat
        }

        let metadata = try entity.roomMetadata ?? {
            let created = try context.insertNew(CDChatRoomMetadata.self)
            created.chatRelativeId = model.chatId.roomId
            return created
        }()

        try model.apply(to: metadata)
        entity.roomMetadata = metadata
    }
}

extension Chat.RoomNameUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) {
        entity.name = name
    }
}

extension Chat.RoomConfigUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) throws {
        entity.config = try config.scaleEncoded()
        entity.configGeneration += 1
    }
}

extension Chat.RoomPreviewUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) throws {
        entity.previewTimestamp = preview.timestamp ?? 0
        entity.badgeKind = preview.badge.storedKind
        entity.badgeCount = preview.badge.storedCount
        entity.previewMessage = try preview.lastMessage?.storedBytes()
        entity.previewUnseenReaction = preview.unseenReaction
    }
}

extension Chat.RoomDeclarations {
    /// The declarations of a stored room. A chat without a room reads as no declarations at all.
    ///
    /// A value whose bytes do not decode reads as not declared, so the room falls back to the host defaults.
    init(entity: CDChatRoomMetadata?) {
        self.init(
            config: entity?.config.flatMap(Chat.RoomConfig.decodedIfValid),
            configGeneration: entity?.configGeneration ?? 0,
            preview: entity.map(Chat.RoomPreview.init(entity:)) ?? .hostDefault
        )
    }
}

// MARK: - Stored forms

/// The stored kinds of a badge: 0 host count, 1 none, 2 count.
private extension Chat.RoomBadge {
    var storedKind: Int16 {
        switch self {
        case .hostDefault: 0
        case .none: 1
        case .unreadCount: 2
        }
    }

    var storedCount: Int32 {
        if case let .unreadCount(count) = self { count } else { 0 }
    }

    init(kind: Int16, count: Int32) {
        switch kind {
        case 1: self = .none
        case 2: self = .unreadCount(count)
        default: self = .hostDefault
        }
    }
}

/// A stored timestamp of zero means that the product set no timestamp.
private extension Chat.RoomPreview {
    init(entity: CDChatRoomMetadata) {
        self.init(
            timestamp: entity.previewTimestamp == 0 ? nil : entity.previewTimestamp,
            badge: Chat.RoomBadge(kind: entity.badgeKind, count: entity.badgeCount),
            lastMessage: entity.previewMessage.flatMap(Chat.RoomPreviewMessage.decodedIfStored),
            unseenReaction: entity.previewUnseenReaction
        )
    }
}

// MARK: - SCALE forms

// Each value has the SCALE layout of its Chat Extension v2 wire type (`v01`), so the stored bytes are the bytes
// that the core encodes. The field and variant order is a wire contract.

extension ScaleDecodable {
    /// The value that `data` holds, or nil when the bytes are not a complete value of this type.
    static func decodedIfValid(_ data: Data) -> Self? {
        guard let decoder = try? ScaleDecoder(data: data), let value = try? Self(scaleDecoder: decoder) else {
            return nil
        }

        return decoder.remained == 0 ? value : nil
    }
}

extension Chat.RoomConfig: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        productSource = try Bool(scaleDecoder: scaleDecoder)
        reactionDetails = try Bool(scaleDecoder: scaleDecoder)
        reportSeen = try Bool(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try productSource.encode(scaleEncoder: scaleEncoder)
        try reactionDetails.encode(scaleEncoder: scaleEncoder)
        try reportSeen.encode(scaleEncoder: scaleEncoder)
    }
}
