import CoreData
import Foundation
import Operation_iOS
import StatementDelivery

/// Delivery state of acceptances: the acceptance message's status and the period of the current copy.
final class ChatAcceptanceOutgoingStore: @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol

    init(storageFacade: StorageFacadeProtocol) {
        databaseService = storageFacade.databaseService
    }
}

extension ChatAcceptanceOutgoingStore: OutgoingStatementStoring {
    func isAwaitingFirstDelivery(_ input: OutgoingChatAcceptanceInput) async throws -> Bool {
        try await databaseService.performRead { context in
            let fetchRequest = CDChatMessage.fetchRequest()
            fetchRequest.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
                .messages(withIds: [input.statementId]),
                .newLocalDeviceChatAcceptances()
            ])
            fetchRequest.fetchLimit = 1

            return try context.count(for: fetchRequest) > 0
        }
    }

    func renewalCandidates() async throws -> [PublishedStatement<OutgoingChatAcceptanceInput>] {
        try await databaseService.performRead { context in
            let fetchRequest = CDChatMessage.fetchRequest()
            fetchRequest.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
                .localDeviceChatAcceptances(),
                .byStatus(.outgoing(.sent))
            ])
            fetchRequest.sortDescriptors = [NSSortDescriptor(key: #keyPath(CDChatMessage.timestamp), ascending: false)]

            let entities = try context.fetch(fetchRequest)
            let periods = try context.outgoingStatementPeriods(for: entities.compactMap(\.messageId))

            return try entities.compactMap { try Self.candidate(from: $0, periods: periods) }
        }
    }

    func markPublished(_ input: OutgoingChatAcceptanceInput, period: UInt32) async throws -> StatementPublication {
        try await databaseService.performWrite { context in
            guard let entity = try Self.fetchMessage(input.statementId, in: context) else {
                return StatementPublication(period: period, isFirst: false)
            }

            let isFirst = entity.status == Chat.LocalMessage.Status.outgoing(.new).rawValue

            try context.setOutgoingStatementPeriod(period, for: input.statementId)

            if isFirst {
                entity.status = Chat.LocalMessage.Status.outgoing(.sent).rawValue
                entity.touchParent()
            }

            return StatementPublication(period: period, isFirst: isFirst)
        }
    }

    func markFailed(_ input: OutgoingChatAcceptanceInput, failure _: OutgoingStatementFailure) async throws {
        try await finish(input, with: .outgoing(.failed))
    }

    // The requester spoke on the device route after the acceptance, or answered one of our device-route messages.
    // Ordered by local insertion, since the peer's timestamps come from the peer's clock.
    func isAcknowledged(_ input: OutgoingChatAcceptanceInput) async throws -> Bool {
        try await databaseService.performRead { context in
            guard
                let acceptance = try Self.fetchMessage(input.statementId, in: context),
                let chat = acceptance.chat
            else {
                return false
            }

            let fetchRequest = CDChatMessage.fetchRequest()
            fetchRequest.predicate = Self.acknowledgementPredicate(in: chat, after: acceptance.order)
            fetchRequest.fetchLimit = 1

            return try context.count(for: fetchRequest) > 0
        }
    }

    func markAcknowledged(_ input: OutgoingChatAcceptanceInput) async throws {
        try await finish(input, with: .outgoing(.delivered))
    }

    func markAbandoned(_ input: OutgoingChatAcceptanceInput) async throws {
        try await finish(input, with: .outgoing(.failed))
    }
}

private extension ChatAcceptanceOutgoingStore {
    func finish(_ input: OutgoingChatAcceptanceInput, with status: Chat.LocalMessage.Status) async throws {
        try await databaseService.performWrite { context in
            try context.deleteOutgoingStatementPublication(for: input.statementId)

            guard let entity = try Self.fetchMessage(input.statementId, in: context) else { return }

            entity.status = status.rawValue
            entity.touchParent()
        }
    }

    static func acknowledgementPredicate(in chat: CDChat, after acceptanceOrder: Int64) -> NSPredicate {
        let incoming = [Chat.LocalMessage.Status.incoming(.new), .incoming(.seen)].map(\.rawValue)
        let delivered = Chat.LocalMessage.Status.outgoing(.delivered).rawValue
        // Inserted by this device while handling the request, not received from the requester.
        let localContentTypes = [
            Chat.LocalMessage.Content.ContentType.versionedChatRequest,
            .chatRequest,
            .deviceAdded
        ].map(\.rawValue)

        return NSCompoundPredicate(andPredicateWithSubpredicates: [
            .messages(in: chat),
            NSPredicate(format: "%K > %lld", #keyPath(CDChatMessage.order), acceptanceOrder),
            NSPredicate(
                format: "(%K IN %@ AND NOT (%K IN %@)) OR %K == %d",
                #keyPath(CDChatMessage.status),
                incoming,
                #keyPath(CDChatMessage.contentType),
                localContentTypes,
                #keyPath(CDChatMessage.status),
                delivered
            )
        ])
    }

    // Without a recorded period the copy counts as stale, so it renews into the current period.
    static func candidate(
        from entity: CDChatMessage,
        periods: [String: UInt32]
    ) throws -> PublishedStatement<OutgoingChatAcceptanceInput>? {
        guard let contactEntity = entity.chat?.contact else { return nil }

        let message = try ChatMessageEntityMapper().transform(entity: entity)
        let contact = try ChatContactMapper().transform(entity: contactEntity)

        guard let input = OutgoingChatAcceptanceInput(message: message, contact: contact) else { return nil }

        return PublishedStatement(
            input: input,
            period: periods[message.messageId] ?? 0,
            createdAt: Date.fromChatTimestamp(message.timestamp)
        )
    }

    static func fetchMessage(_ messageId: String, in context: NSManagedObjectContext) throws -> CDChatMessage? {
        let fetchRequest = CDChatMessage.fetchRequest()
        fetchRequest.predicate = .messages(withIds: [messageId])
        fetchRequest.fetchLimit = 1

        return try context.fetch(fetchRequest).first
    }
}
