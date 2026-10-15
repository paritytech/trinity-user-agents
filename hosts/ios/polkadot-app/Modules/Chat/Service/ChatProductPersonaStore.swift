import AsyncExtensions
import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

protocol ChatProductPersonaStoring: Sendable {
    /// Keeps a persona of a product. A persona that exists already gets the new name and icon. Throws
    /// ``ChatProductPersonaError/tooMany`` for a new persona of a product that has
    /// ``ChatProductPersonaStore/maxPerProduct``.
    func register(_ persona: Chat.ProductPersona) async throws -> Chat.PersonaRegistration

    /// The personas of a product, by bot id.
    func personas(of productId: String) async throws -> [Chat.ProductPersona]

    /// Gives the personas of a product now, and then after each change.
    func personaUpdates(of productId: String) -> AnyAsyncSequence<[Chat.ProductPersona]>
}

enum ChatProductPersonaError: Error, Equatable {
    /// The product has the largest number of personas.
    case tooMany(max: Int)
}

extension Chat {
    /// The answer to `register_bot` (`ChatBotRegistrationStatus`).
    enum PersonaRegistration: Equatable {
        case new
        case exists
    }
}

/// Keeps the personas of the products in `CDProductChatPersona` (07-LATER §5). A product speaks as a persona in its
/// own rooms with the participant id `bot:<bot_id>`, and the room screen draws the name and the icon of the persona.
final class ChatProductPersonaStore: ChatProductPersonaStoring, @unchecked Sendable {
    /// `CHAT_PERSONAS_MAX` of the core. The core checks the bound too.
    static let maxPerProduct = 8

    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol) {
        self.storageFacade = storageFacade
    }

    func register(_ persona: Chat.ProductPersona) async throws -> Chat.PersonaRegistration {
        try await storageFacade.databaseService.performWrite { context in
            let identifier = Chat.ProductPersona.identifier(productId: persona.productId, botId: persona.botId)
            let existing: CDProductChatPersona? = try context.first(for: .persona(identifier: identifier))

            if existing == nil {
                let request: NSFetchRequest<CDProductChatPersona> = CDProductChatPersona.fetchRequest()
                request.predicate = .personas(of: persona.productId)
                guard try context.count(for: request) < Self.maxPerProduct else {
                    throw ChatProductPersonaError.tooMany(max: Self.maxPerProduct)
                }
            }

            let entity = try existing ?? context.insertNew(CDProductChatPersona.self)
            ChatProductPersonaMapper().update(entity, from: persona)
            return existing == nil ? .new : .exists
        }
    }

    func personas(of productId: String) async throws -> [Chat.ProductPersona] {
        try await storageFacade.databaseService.performRead { context in
            let request: NSFetchRequest<CDProductChatPersona> = CDProductChatPersona.fetchRequest()
            request.predicate = .personas(of: productId)

            let mapper = ChatProductPersonaMapper()
            return try context.fetch(request)
                .compactMap { try? mapper.transform(entity: $0) }
                .sorted { $0.botId < $1.botId }
        }
    }

    func personaUpdates(of productId: String) -> AnyAsyncSequence<[Chat.ProductPersona]> {
        storageFacade.subscribeSnapshot(
            mapper: AnyCoreDataMapper(ChatProductPersonaMapper()),
            filter: .personas(of: productId),
            transform: { $0.sorted { $0.botId < $1.botId } }
        )
    }
}

extension Chat.ProductPersona: Identifiable {
    /// The participant id under which the product speaks as the persona.
    static let participantPrefix = "bot:"

    var identifier: String {
        Self.identifier(productId: productId, botId: botId)
    }

    var participantId: String {
        Self.participantPrefix + botId
    }

    static func identifier(productId: String, botId: String) -> String {
        "\(productId)/\(botId)"
    }

    /// The bot id of a participant id `bot:<bot_id>`, or nil for another participant id.
    static func botId(ofParticipant participantId: String) -> String? {
        guard participantId.hasPrefix(participantPrefix) else { return nil }
        let botId = participantId.dropFirst(participantPrefix.count)
        return botId.isEmpty ? nil : String(botId)
    }
}

/// Maps a persona to its row. The row keeps the icon as the UTF-8 bytes of its source: an `https` address or an
/// inline `data:` address, as the product gave it.
final class ChatProductPersonaMapper: CoreDataMapperProtocol {
    typealias DataProviderModel = Chat.ProductPersona
    typealias CoreDataEntity = CDProductChatPersona

    enum MapperError: Error {
        case missingRequiredData(String)
    }

    var entityIdentifierFieldName: String {
        #keyPath(CDProductChatPersona.identifier)
    }

    func transform(entity: CDProductChatPersona) throws -> Chat.ProductPersona {
        guard let productId = entity.productId, let botId = entity.botId else {
            throw MapperError.missingRequiredData("productId, botId")
        }

        return Chat.ProductPersona(
            productId: productId,
            botId: botId,
            name: entity.name ?? botId,
            icon: entity.iconData.flatMap { String(data: $0, encoding: .utf8) }
        )
    }

    func populate(
        entity: CDProductChatPersona,
        from model: Chat.ProductPersona,
        using _: NSManagedObjectContext
    ) throws {
        update(entity, from: model)
    }

    func update(_ entity: CDProductChatPersona, from persona: Chat.ProductPersona) {
        entity.identifier = persona.identifier
        entity.productId = persona.productId
        entity.botId = persona.botId
        entity.name = persona.name
        entity.iconData = persona.icon.flatMap { $0.isEmpty ? nil : Data($0.utf8) }
    }
}

extension NSPredicate {
    static func persona(identifier: String) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDProductChatPersona.identifier), identifier)
    }
}
