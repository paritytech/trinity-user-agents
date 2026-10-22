import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

/// Reads what a product room holds: the declarations of the room and its name.
protocol ProductRoomReading: Sendable {
    /// The declarations of a stored room, or nil when no such room exists.
    func declarations(of chatId: Chat.Id) async -> Chat.RoomDeclarations?

    /// The name that the product gave the room, or nil when no such room exists.
    func name(of chatId: Chat.Id) async -> String?
}

final class ProductRoomReader: ProductRoomReading, @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol

    init(storageFacade: StorageFacadeProtocol) {
        databaseService = storageFacade.databaseService
    }

    func declarations(of chatId: Chat.Id) async -> Chat.RoomDeclarations? {
        try? await databaseService.performRead { context in
            let chat: CDChat? = try context.first(for: .chat(for: chatId.rawRepresentation))
            return chat.map { Chat.RoomDeclarations(entity: $0.roomMetadata) }
        }
    }

    func name(of chatId: Chat.Id) async -> String? {
        try? await databaseService.performRead { context in
            let chat: CDChat? = try context.first(for: .chat(for: chatId.rawRepresentation))
            return chat?.roomMetadata?.name
        }
    }
}
