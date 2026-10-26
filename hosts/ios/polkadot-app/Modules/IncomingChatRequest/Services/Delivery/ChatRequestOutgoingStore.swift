import CoreData
import Foundation
import MessageExchangeKit
import Operation_iOS
import StatementDelivery

/// Delivery state of outgoing chat requests: the welcome message's status, the request's status and the period of the
/// current copy.
final class ChatRequestOutgoingStore: @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol

    init(storageFacade: StorageFacadeProtocol) {
        databaseService = storageFacade.databaseService
    }
}

extension ChatRequestOutgoingStore: OutgoingStatementStoring {
    // Still ours to deliver: outgoing, its welcome message unsent, and still the contact's request.
    func isAwaitingFirstDelivery(_ input: OutgoingChatRequestInput) async throws -> Bool {
        try await databaseService.performRead { context in
            guard let request = try Self.fetchRequest(input.statementId, in: context) else { return false }

            return Self.isPending(request) &&
                request.message?.status == Chat.LocalMessage.Status.outgoing(.new).rawValue
        }
    }

    func renewalCandidates() async throws -> [PublishedStatement<OutgoingChatRequestInput>] {
        try await databaseService.performRead { context in
            let fetchRequest = CDChatContact.fetchRequest()
            fetchRequest.predicate = Self.renewablePredicate()
            fetchRequest.sortDescriptors = [
                NSSortDescriptor(key: #keyPath(CDChatContact.chatRequest.timestamp), ascending: false)
            ]

            let mapper = ChatContactMapper()
            let contacts = try context.fetch(fetchRequest).map { try mapper.transform(entity: $0) }
            let periods = try context.outgoingStatementPeriods(for: contacts.compactMap(\.chatRequest?.requestId))

            return contacts.compactMap { Self.candidate(from: $0, periods: periods) }
        }
    }

    func markPublished(_ input: OutgoingChatRequestInput, period: UInt32) async throws -> StatementPublication {
        try await databaseService.performWrite { context in
            guard let request = try Self.fetchRequest(input.statementId, in: context) else {
                return StatementPublication(period: period, isFirst: false)
            }

            let isFirst = request.message?.status == Chat.LocalMessage.Status.outgoing(.new).rawValue

            try context.setOutgoingStatementPeriod(period, for: input.statementId)

            if isFirst {
                request.message?.status = Chat.LocalMessage.Status.outgoing(.sent).rawValue
                request.message?.touchParent()
                request.touchParent()
            }

            return StatementPublication(period: period, isFirst: isFirst)
        }
    }

    func markFailed(_ input: OutgoingChatRequestInput, failure _: OutgoingStatementFailure) async throws {
        try await databaseService.performWrite { context in
            try context.deleteOutgoingStatementPublication(for: input.statementId)

            guard let request = try Self.fetchRequest(input.statementId, in: context) else { return }

            request.message?.status = Chat.LocalMessage.Status.outgoing(.failed).rawValue
            request.message?.touchParent()
            request.touchParent()
        }
    }

    // Accepted, replaced by another request or the contact is gone.
    func isAcknowledged(_ input: OutgoingChatRequestInput) async throws -> Bool {
        try await databaseService.performRead { context in
            guard let request = try Self.fetchRequest(input.statementId, in: context) else { return true }

            return !Self.isPending(request)
        }
    }

    func markAcknowledged(_ input: OutgoingChatRequestInput) async throws {
        try await databaseService.performWrite { context in
            try context.deleteOutgoingStatementPublication(for: input.statementId)
        }
    }

    func markAbandoned(_ input: OutgoingChatRequestInput) async throws {
        try await markFailed(input, failure: .terminal(ChatRequestOutgoingStoreError.abandoned))
    }
}

enum ChatRequestOutgoingStoreError: Error {
    case abandoned
}

private extension ChatRequestOutgoingStore {
    static func isPending(_ request: CDChatRequest) -> Bool {
        request.status == Chat.RequestStatus.outgoing.rawValue && request.contact?.chatRequest == request
    }

    static func renewablePredicate() -> NSPredicate {
        let sentStatuses = [
            Chat.LocalMessage.Status.outgoing(.sent).rawValue,
            Chat.LocalMessage.Status.outgoing(.delivered).rawValue
        ]

        return NSPredicate(
            format: "%K == %d AND %K IN %@",
            #keyPath(CDChatContact.chatRequest.status),
            Chat.RequestStatus.outgoing.rawValue,
            #keyPath(CDChatContact.chatRequest.message.status),
            sentStatuses
        )
    }

    // Without a recorded period (e.g. sent before the publication table existed) the copy counts as stale,
    // so it renews into the current period.
    static func candidate(
        from contact: Chat.Contact,
        periods: [String: UInt32]
    ) -> PublishedStatement<OutgoingChatRequestInput>? {
        guard
            let request = contact.chatRequest,
            let localMessage = request.message,
            let message = Chat.RequestMessage(localMessage: localMessage)
        else {
            return nil
        }

        return PublishedStatement(
            input: OutgoingChatRequestInput(message: message, session: contact.toMessageExchangeSessionRequest()),
            period: periods[request.requestId] ?? 0,
            createdAt: Date.fromChatTimestamp(message.timestamp)
        )
    }

    static func fetchRequest(_ requestId: String, in context: NSManagedObjectContext) throws -> CDChatRequest? {
        let fetchRequest = CDChatRequest.fetchRequest()
        fetchRequest.predicate = .chatRequestById(requestId)
        fetchRequest.fetchLimit = 1

        return try context.fetch(fetchRequest).first
    }
}
