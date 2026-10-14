import CoreData
import DurableTransactions
import Foundation
import Operation_iOS

/// Writes one chat message and lets a caller make something else durable in the same transaction.
///
/// The stock `CoreDataRepository` cannot do this: its context-taking `save(models:in:)` is internal to
/// Operation-iOS, so the app cannot drive it with a transaction of its own. This opens the write
/// itself and reuses ``ChatMessageEntityMapper`` for the row, so grouping, chat resolution and
/// `touchParent` stay in one place.
protocol ChatMessageTransactionalStoring: Sendable {
    /// `onSaved` runs **inside** the transaction that persists the message, so a caller with a fact
    /// that must become durable exactly when the message does — a payment's keys are now on their way —
    /// can commit it there and nowhere else. Throwing from it rolls the message back with it.
    ///
    /// Synchronous by necessity: the body runs inside a CoreData `perform` block, which cannot suspend.
    func save(
        _ message: Chat.LocalMessage,
        onSaved: @escaping (any DurableTxRegistrationScope) throws -> Void
    ) async throws
}

final class ChatMessageTransactionalStore: ChatMessageTransactionalStoring, @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol
    private let mapper = ChatMessageEntityMapper()

    init(storageFacade: StorageFacadeProtocol) {
        databaseService = storageFacade.databaseService
    }

    func save(
        _ message: Chat.LocalMessage,
        onSaved: @escaping (any DurableTxRegistrationScope) throws -> Void
    ) async throws {
        try await databaseService.performWrite { [mapper] context in
            let entity = try Self.entity(for: message, in: context)
            try mapper.populate(entity: entity, from: message, using: context)

            // Inside the transaction, before the row is visible: whatever carries these keys becomes
            // durable exactly when the message does.
            try onSaved(CoreDataRegistrationScope(context: context))
        }
    }
}

private extension ChatMessageTransactionalStore {
    /// Fetch-or-insert by message id, the way the repository's save does.
    static func entity(
        for message: Chat.LocalMessage,
        in context: NSManagedObjectContext
    ) throws -> CDChatMessage {
        let existing: CDChatMessage? = try context.first(
            for: NSPredicate(format: "%K == %@", #keyPath(CDChatMessage.messageId), message.messageId)
        )

        return try existing ?? context.insertNew(CDChatMessage.self)
    }
}
