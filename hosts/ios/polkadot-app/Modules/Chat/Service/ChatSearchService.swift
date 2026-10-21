import CoreData
import Foundation
import Operation_iOS

/// A message that a search of the stored chats found (07-LATER §8.2).
struct ChatSearchHit: Equatable {
    let chatId: Chat.Id
    /// The stored id of the message to show. A hit on an edit gives the message that the edit changed.
    let messageId: Chat.MessageId
    let timestamp: Chat.Timestamp
    /// The text of the message, as the user reads it.
    let text: String
    let isOutgoing: Bool
    /// The participant of a product room that wrote the message, when it is not the user or the product.
    let authorParticipantId: String?
}

/// Where the next page of a search starts: after the last row of the page, newest first.
struct ChatSearchCursor: Equatable {
    let timestamp: Int64
    let messageId: String
}

struct ChatSearchPage: Equatable {
    /// Newest first.
    let hits: [ChatSearchHit]
    /// Nil when no older matches exist.
    let next: ChatSearchCursor?
}

/// Searches the text of the messages that the app stores: one room for the in-room search, many chats for the
/// search of the chat list. A product-source room keeps no messages here, so its search goes to the product.
protocol ChatSearchServicing: Sendable {
    /// One page of the messages of a chat that hold `text`, case and diacritics ignored, newest first.
    func searchMessages(
        matching text: String,
        in chatId: Chat.Id,
        after cursor: ChatSearchCursor?
    ) async throws -> ChatSearchPage

    /// The newest messages of the chats that hold `text`, case and diacritics ignored.
    func searchMessages(matching text: String, in chatIds: Set<Chat.Id>, limit: Int) async throws -> [ChatSearchHit]
}

final class ChatSearchService: ChatSearchServicing, @unchecked Sendable {
    /// The size of a page of the in-room search.
    static let pageSize = 50
    /// How many rows the fill of the missing texts writes in one transaction.
    static let fillBatchSize = 500

    private let databaseService: CoreDataServiceProtocol
    private let filling = MissingTextFill()

    init(databaseService: CoreDataServiceProtocol = UserDataStorageFacade.shared.databaseService) {
        self.databaseService = databaseService
    }

    func searchMessages(
        matching text: String,
        in chatId: Chat.Id,
        after cursor: ChatSearchCursor?
    ) async throws -> ChatSearchPage {
        let query = ChatSearchText.normalized(text)
        guard !query.isEmpty else { return ChatSearchPage(hits: [], next: nil) }

        try await fillMissingTexts()

        let limit = Self.pageSize
        let identifier = chatId.rawRepresentation
        return try await databaseService.performRead { context in
            guard let chat = try context.fetchChat(identifier) else {
                return ChatSearchPage(hits: [], next: nil)
            }

            var predicates = [NSPredicate.messages(in: chat), .searchableText(containing: query)]
            if let cursor {
                predicates.append(.olderThan(cursor))
            }
            let rows = try context.fetchSearchRows(
                NSCompoundPredicate(andPredicateWithSubpredicates: predicates),
                limit: limit
            )
            let next = rows.count == limit ? rows.last.flatMap(ChatSearchCursor.init(row:)) : nil
            return try ChatSearchPage(hits: Self.hits(of: rows, chatId: chatId, in: context), next: next)
        }
    }

    func searchMessages(matching text: String, in chatIds: Set<Chat.Id>, limit: Int) async throws -> [ChatSearchHit] {
        let query = ChatSearchText.normalized(text)
        guard !query.isEmpty, !chatIds.isEmpty else { return [] }

        try await fillMissingTexts()

        let identifiers = chatIds.map(\.rawRepresentation)
        return try await databaseService.performRead { context in
            let predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
                NSPredicate(format: "%K IN %@", #keyPath(CDChatMessage.chat.identifier), identifiers),
                .searchableText(containing: query)
            ])
            let rows = try context.fetchSearchRows(predicate, limit: limit)
            return try Self.hits(of: rows, chatId: nil, in: context)
        }
    }
}

private extension ChatSearchService {
    /// Writes `searchableText` for the rows that a build before search stored, once in each run of the app.
    func fillMissingTexts() async throws {
        guard await filling.start() else { return }

        do {
            while try await fillMissingTextBatch() == Self.fillBatchSize {}
            await filling.finish(success: true)
        } catch {
            await filling.finish(success: false)
            throw error
        }
    }

    func fillMissingTextBatch() async throws -> Int {
        let batchSize = Self.fillBatchSize
        return try await databaseService.performWrite { context in
            let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
            request.predicate = NSPredicate(format: "%K == nil", #keyPath(CDChatMessage.searchableText))
            request.fetchLimit = batchSize

            let rows = try context.fetch(request)
            for row in rows {
                let content = try? ChatMessageEntityMapper.getContent(from: row)
                row.searchableText = content.map(ChatSearchText.stored(for:)) ?? ""
            }
            return rows.count
        }
    }

    /// The hits of the rows, newest first. A hit on an edit gives the edited message once, and a hit whose message
    /// the user deleted gives nothing.
    static func hits(
        of rows: [CDChatMessage],
        chatId: Chat.Id?,
        in context: NSManagedObjectContext
    ) throws -> [ChatSearchHit] {
        var seen: Set<Chat.MessageId> = []
        var hits: [ChatSearchHit] = []

        for row in rows {
            guard
                let hit = hit(of: row, chatId: chatId, in: context),
                seen.insert(hit.messageId).inserted
            else {
                continue
            }
            hits.append(hit)
        }

        return hits
    }

    static func hit(of row: CDChatMessage, chatId: Chat.Id?, in context: NSManagedObjectContext) -> ChatSearchHit? {
        guard
            let content = try? ChatMessageEntityMapper.getContent(from: row),
            let text = content.searchSource,
            let chatId = chatId ?? row.chat?.identifier.flatMap(Chat.Id.fromRawRepresentation),
            let status = Chat.LocalMessage.Status(rawValue: row.status)
        else {
            return nil
        }

        var messageId = row.messageId
        if case let .edited(edit) = content {
            let edited = try? context.fetchMessage(edit.messageId)
            guard let edited, edited.contentType != Int16(Chat.LocalMessage.Content.ContentType.deleted.rawValue) else {
                return nil
            }
            messageId = edited.messageId
        }

        guard let messageId else { return nil }

        return ChatSearchHit(
            chatId: chatId,
            messageId: messageId,
            timestamp: UInt64(bitPattern: row.timestamp),
            text: text,
            isOutgoing: status.isOutgoing,
            authorParticipantId: row.originParticipantId
        )
    }
}

/// Runs the fill of the missing texts once, and again after a failure.
private actor MissingTextFill {
    private var state = State.notStarted
    private var waiters: [CheckedContinuation<Bool, Never>] = []

    private enum State {
        case notStarted
        case running
        case done
    }

    /// True when the caller must run the fill. A caller that comes while the fill runs waits for it.
    func start() async -> Bool {
        switch state {
        case .done:
            return false
        case .notStarted:
            state = .running
            return true
        case .running:
            return await withCheckedContinuation { waiters.append($0) }
        }
    }

    func finish(success: Bool) {
        state = success ? .done : .notStarted
        let waiting = waiters
        waiters = []
        waiting.forEach { $0.resume(returning: false) }
    }
}

private extension ChatSearchCursor {
    init?(row: CDChatMessage) {
        guard let messageId = row.messageId else { return nil }
        self.init(timestamp: row.timestamp, messageId: messageId)
    }
}

private extension NSPredicate {
    static func searchableText(containing query: String) -> NSPredicate {
        NSPredicate(format: "%K CONTAINS %@", #keyPath(CDChatMessage.searchableText), query)
    }

    static func olderThan(_ cursor: ChatSearchCursor) -> NSPredicate {
        NSPredicate(
            format: "%K < %lld OR (%K == %lld AND %K < %@)",
            #keyPath(CDChatMessage.timestamp),
            cursor.timestamp,
            #keyPath(CDChatMessage.timestamp),
            cursor.timestamp,
            #keyPath(CDChatMessage.messageId),
            cursor.messageId
        )
    }
}

private extension NSManagedObjectContext {
    func fetchChat(_ identifier: String) throws -> CDChat? {
        let request: NSFetchRequest<CDChat> = CDChat.fetchRequest()
        request.predicate = .chat(for: identifier)
        request.fetchLimit = 1
        return try fetch(request).first
    }

    func fetchMessage(_ messageId: String) throws -> CDChatMessage? {
        let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
        request.predicate = NSPredicate(format: "%K == %@", #keyPath(CDChatMessage.messageId), messageId)
        request.fetchLimit = 1
        return try fetch(request).first
    }

    func fetchSearchRows(_ predicate: NSPredicate, limit: Int) throws -> [CDChatMessage] {
        let request: NSFetchRequest<CDChatMessage> = CDChatMessage.fetchRequest()
        request.predicate = predicate
        request.sortDescriptors = [
            NSSortDescriptor(key: #keyPath(CDChatMessage.timestamp), ascending: false),
            NSSortDescriptor(key: #keyPath(CDChatMessage.messageId), ascending: false)
        ]
        request.fetchLimit = limit
        request.relationshipKeyPathsForPrefetching = [#keyPath(CDChatMessage.content)]
        return try fetch(request)
    }
}
