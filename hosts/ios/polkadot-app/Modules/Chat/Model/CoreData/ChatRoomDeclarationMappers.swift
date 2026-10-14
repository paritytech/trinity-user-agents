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

extension Chat.RoomDescriptionUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) {
        entity.roomDescription = description
        entity.avatarKind = avatar.storedKind
        entity.avatarRef = avatar.storedRef
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
    func apply(to entity: CDChatRoomMetadata) {
        entity.previewOrder = preview.order.rawValue
        entity.previewTimestamp = preview.timestamp ?? 0
        entity.badgeKind = preview.badge.storedKind
        entity.badgeCount = preview.badge.storedCount
        entity.badgeImageRef = preview.badge.storedImageRef
        entity.previewText = preview.text
        entity.previewOngoing = preview.ongoing
    }
}

extension Chat.RoomToolbarUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) throws {
        entity.toolbar = try actions.scaleEncoded()
    }
}

extension Chat.RoomSuggestionsUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) throws {
        entity.suggestions = try suggestions.scaleEncoded()
    }
}

extension Chat.RoomCommandsUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) throws {
        entity.commands = try commands.scaleEncoded()
    }
}

extension Chat.RoomFooterConfigUpdate: ChatRoomDeclarationUpdating {
    func apply(to entity: CDChatRoomMetadata) throws {
        entity.footerConfig = try footer.scaleEncoded()
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

        let declarations = Chat.RoomDeclarations(entity: entity.roomMetadata)
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

extension Chat.RoomDeclarations {
    /// The declarations of a stored room. A chat without a room reads as no declarations at all.
    ///
    /// A value whose bytes do not decode reads as not declared, so the room falls back to the host defaults.
    init(entity: CDChatRoomMetadata?) {
        self.init(
            description: entity?.roomDescription,
            avatar: Chat.RoomImage(avatarKind: entity?.avatarKind ?? 0, ref: entity?.avatarRef),
            config: entity?.config.flatMap(Chat.RoomConfig.decodedIfValid),
            configGeneration: entity?.configGeneration ?? 0,
            preview: entity.map(Chat.RoomPreview.init(entity:)) ?? .hostDefault,
            toolbar: entity?.toolbar.flatMap([Chat.RoomToolbarAction].decodedIfValid),
            suggestions: entity?.suggestions.flatMap([Chat.RoomSuggestion].decodedIfValid),
            commands: entity?.commands.flatMap([Chat.RoomCommand].decodedIfValid),
            footerConfig: entity?.footerConfig.flatMap(Chat.RoomFooterConfig.decodedIfValid)
        )
    }
}

// MARK: - Stored forms

/// The stored kinds of an avatar: 0 none, 1 inline bytes, 2 preimage key. `avatarRef` holds the bytes or the key.
private extension Chat.RoomImage? {
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
        case let .inline(data): data
        case let .preimage(key): key
        }
    }
}

private extension Chat.RoomImage {
    init?(avatarKind: Int16, ref: Data?) {
        switch (avatarKind, ref) {
        case let (1, .some(ref)): self = .inline(ref)
        case let (2, .some(ref)): self = .preimage(key: ref)
        default: return nil
        }
    }
}

/// The stored kinds of a badge: 0 host count, 1 none, 2 count, 3 preimage image, 4 inline image.
/// `badgeImageRef` holds the key or the bytes of the image.
private extension Chat.RoomBadge {
    var storedKind: Int16 {
        switch self {
        case .hostDefault: 0
        case .none: 1
        case .unreadCount: 2
        case .image(.preimage): 3
        case .image(.inline): 4
        }
    }

    var storedCount: Int32 {
        if case let .unreadCount(count) = self { count } else { 0 }
    }

    var storedImageRef: Data? {
        switch self {
        case let .image(.preimage(key)): key
        case let .image(.inline(data)): data
        case .hostDefault,
             .none,
             .unreadCount: nil
        }
    }

    init(kind: Int16, count: Int32, imageRef: Data?) {
        switch (kind, imageRef) {
        case (1, _): self = .none
        case (2, _): self = .unreadCount(count)
        case let (3, .some(ref)): self = .image(.preimage(key: ref))
        case let (4, .some(ref)): self = .image(.inline(ref))
        default: self = .hostDefault
        }
    }
}

/// A stored timestamp of zero means that the product set no timestamp.
private extension Chat.RoomPreview {
    init(entity: CDChatRoomMetadata) {
        self.init(
            order: Order(rawValue: entity.previewOrder) ?? .byTimestamp,
            timestamp: entity.previewTimestamp == 0 ? nil : entity.previewTimestamp,
            badge: Chat.RoomBadge(kind: entity.badgeKind, count: entity.badgeCount, imageRef: entity.badgeImageRef),
            text: entity.previewText,
            ongoing: entity.previewOngoing
        )
    }
}

// MARK: - SCALE forms

// Each value has the SCALE layout of its Chat Extension v2 wire type (`v01` and, for the footer, `v02`), so
// the stored bytes are the bytes that the core encodes. The field and variant order is a wire contract.

private extension ScaleDecodable {
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
        participants = try Bool(scaleDecoder: scaleDecoder)
        messageActions = try Bool(scaleDecoder: scaleDecoder)
        participantActions = try Bool(scaleDecoder: scaleDecoder)
        allowedReactions = try ScaleOption<[String]>(scaleDecoder: scaleDecoder).value
        placeholder = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
        reportSeen = try Bool(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try productSource.encode(scaleEncoder: scaleEncoder)
        try reactionDetails.encode(scaleEncoder: scaleEncoder)
        try participants.encode(scaleEncoder: scaleEncoder)
        try messageActions.encode(scaleEncoder: scaleEncoder)
        try participantActions.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: allowedReactions).encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: placeholder).encode(scaleEncoder: scaleEncoder)
        try reportSeen.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.ProductResource: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        switch try UInt8(scaleDecoder: scaleDecoder) {
        case 0:
            let mimeType = try String(scaleDecoder: scaleDecoder)
            self = try .inline(mimeType: mimeType, data: Data(scaleDecoder: scaleDecoder))
        case 1:
            self = try .preimage(key: Data(scaleDecoder: scaleDecoder))
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .inline(mimeType, data):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try mimeType.encode(scaleEncoder: scaleEncoder)
            try data.encode(scaleEncoder: scaleEncoder)
        case let .preimage(key):
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            try key.encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.ProductActionHandler: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        switch try UInt8(scaleDecoder: scaleDecoder) {
        case 0: self = try .deeplink(url: String(scaleDecoder: scaleDecoder))
        case 1: self = .callback
        default: throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .deeplink(url):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try url.encode(scaleEncoder: scaleEncoder)
        case .callback:
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
        }
    }
}

extension Chat.RoomToolbarAction: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        actionId = try String(scaleDecoder: scaleDecoder)
        label = try String(scaleDecoder: scaleDecoder)
        icon = try Chat.ProductResource(scaleDecoder: scaleDecoder)
        handler = try Chat.ProductActionHandler(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try actionId.encode(scaleEncoder: scaleEncoder)
        try label.encode(scaleEncoder: scaleEncoder)
        try icon.encode(scaleEncoder: scaleEncoder)
        try handler.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RoomSuggestion: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        suggestionId = try String(scaleDecoder: scaleDecoder)
        text = try String(scaleDecoder: scaleDecoder)
        sendsText = try Bool(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try suggestionId.encode(scaleEncoder: scaleEncoder)
        try text.encode(scaleEncoder: scaleEncoder)
        try sendsText.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RoomCommand: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        name = try String(scaleDecoder: scaleDecoder)
        description = try String(scaleDecoder: scaleDecoder)
        argumentHint = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try name.encode(scaleEncoder: scaleEncoder)
        try description.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: argumentHint).encode(scaleEncoder: scaleEncoder)
    }
}

/// The wire destination is a `[u8; 32]`, which SCALE encodes as 32 bytes with no length prefix.
extension Chat.RoomInputPayment: ScaleCodable {
    private static let destinationLength = 32

    init(scaleDecoder: any ScaleDecoding) throws {
        guard try UInt8(scaleDecoder: scaleDecoder) == 0 else {
            throw ScaleCodingError.unexpectedDecodedValue
        }

        self = try .enabled(destination: scaleDecoder.readAndConfirm(count: Self.destinationLength))
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .enabled(destination):
            guard destination.count == Self.destinationLength else {
                throw ScaleCodingError.unexpectedDecodedValue
            }

            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            scaleEncoder.appendRaw(data: destination)
        }
    }
}

extension Chat.RoomInputConfig: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        placeholder = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
        photo = try Bool(scaleDecoder: scaleDecoder)
        video = try Bool(scaleDecoder: scaleDecoder)
        file = try Bool(scaleDecoder: scaleDecoder)
        payment = try ScaleOption<Chat.RoomInputPayment>(scaleDecoder: scaleDecoder).value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try ScaleOption(value: placeholder).encode(scaleEncoder: scaleEncoder)
        try photo.encode(scaleEncoder: scaleEncoder)
        try video.encode(scaleEncoder: scaleEncoder)
        try file.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: payment).encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RoomFooterConfig: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        switch try UInt8(scaleDecoder: scaleDecoder) {
        case 0: self = try .input(Chat.RoomInputConfig(scaleDecoder: scaleDecoder))
        case 1: self = .empty
        case 2: self = .custom
        default: throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .input(input):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try input.encode(scaleEncoder: scaleEncoder)
        case .empty:
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
        case .custom:
            try UInt8(2).encode(scaleEncoder: scaleEncoder)
        }
    }
}
