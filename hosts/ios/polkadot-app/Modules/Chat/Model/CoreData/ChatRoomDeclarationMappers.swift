import CoreData
import Foundation
import Operation_iOS

/// A partial update of the declarations of one room. Each update writes only its own attributes.
protocol ChatRoomDeclarationUpdating: Identifiable {
    var chatId: Chat.Id { get }

    func apply(to entity: CDChatRoomMetadata)
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

        model.apply(to: metadata)
        entity.roomMetadata = metadata
    }
}

extension Chat.RoomDescriptionUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) {
        entity.roomDescription = description
        entity.avatarKind = avatar.storedKind
        entity.avatarRef = avatar.storedRef
    }
}

extension Chat.RoomConfigUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) {
        entity.config = config
        entity.configGeneration += 1
    }
}

extension Chat.RoomPreviewUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) {
        entity.previewOrder = preview.order.rawValue
        entity.previewTimestamp = preview.timestamp
        entity.badgeKind = preview.badge.storedKind
        entity.badgeCount = preview.badge.storedCount
        entity.badgeImageRef = preview.badge.storedImageRef
        entity.previewText = preview.text
        entity.previewOngoing = preview.ongoing
    }
}

extension Chat.RoomListUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) {
        switch kind {
        case .toolbar: entity.toolbar = value
        case .suggestions: entity.suggestions = value
        case .commands: entity.commands = value
        case .footer: entity.footerConfig = value
        }
    }
}

extension Chat.StoredRoomDeclarations: Identifiable {
    var identifier: String {
        chatId
    }
}

/// Reads the declarations of stored rooms. A chat without a room reads as no declarations at all.
final class ChatRoomDeclarationsMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = Chat.StoredRoomDeclarations
    typealias CoreDataEntity = CDChat
}

extension ChatRoomDeclarationsMapper: CoreDataMapperProtocol {
    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        guard let identifier = entity.identifier else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDChat.identifier))
        }

        let metadata = entity.roomMetadata
        let declarations = Chat.RoomDeclarations(
            description: metadata?.roomDescription,
            avatar: Chat.RoomAvatar(kind: metadata?.avatarKind ?? 0, ref: metadata?.avatarRef),
            config: metadata?.config,
            configGeneration: metadata?.configGeneration ?? 0,
            preview: metadata.map(Chat.RoomPreview.init(entity:)) ?? .hostDefault,
            toolbar: metadata?.toolbar,
            suggestions: metadata?.suggestions,
            commands: metadata?.commands,
            footerConfig: metadata?.footerConfig
        )

        return Chat.StoredRoomDeclarations(chatId: identifier, declarations: declarations)
    }

    func populate(
        entity _: CoreDataEntity,
        from _: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        throw CoreDataMapperError.unsupported
    }
}

// MARK: - Stored forms

private extension Chat.RoomAvatar {
    var storedKind: Int16 {
        switch self {
        case .none: 0
        case .inline: 1
        case .preimage: 2
        }
    }

    var storedRef: Data? {
        switch self {
        case .none: nil
        case let .inline(storeKey): storeKey
        case let .preimage(key): key
        }
    }

    init(kind: Int16, ref: Data?) {
        switch (kind, ref) {
        case let (1, .some(ref)): self = .inline(storeKey: ref)
        case let (2, .some(ref)): self = .preimage(key: ref)
        default: self = .none
        }
    }
}

private extension Chat.RoomBadge {
    var storedKind: Int16 {
        switch self {
        case .hostDefault: 0
        case .none: 1
        case .unreadCount: 2
        case .image: 3
        }
    }

    var storedCount: Int32 {
        if case let .unreadCount(count) = self { count } else { 0 }
    }

    var storedImageRef: Data? {
        if case let .image(ref) = self { ref } else { nil }
    }

    init(kind: Int16, count: Int32, imageRef: Data?) {
        switch (kind, imageRef) {
        case (1, _): self = .none
        case (2, _): self = .unreadCount(count)
        case let (3, .some(ref)): self = .image(ref: ref)
        default: self = .hostDefault
        }
    }
}

private extension Chat.RoomPreview {
    init(entity: CDChatRoomMetadata) {
        self.init(
            order: Order(rawValue: entity.previewOrder) ?? .byTimestamp,
            timestamp: entity.previewTimestamp,
            badge: Chat.RoomBadge(kind: entity.badgeKind, count: entity.badgeCount, imageRef: entity.badgeImageRef),
            text: entity.previewText,
            ongoing: entity.previewOngoing
        )
    }
}
