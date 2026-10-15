import CoreData
import Foundation
import MessageExchangeKit
import Operation_iOS
import StructuredConcurrency

protocol ChatRequestDeliveryStoring {
    /// Still ours to deliver: outgoing, its welcome message unsent, and still the contact's request — the peer's
    /// own request may have been accepted, or the contact removed, in the meantime.
    func isAwaitingDelivery(requestId: String) async throws -> Bool

    /// Marks the welcome message sent and records which period's account delivered it, in one write.
    func markDelivered(requestId: String, anonymousPeriod: UInt32) async throws

    func markFailed(requestId: String) async throws
}

struct ChatRequestRenewalCandidate {
    let message: Chat.RequestMessage
    let session: MessageExchange.SessionRequest
    let period: UInt32
}

protocol ChatRequestRenewalStoring {
    /// Newest first.
    func renewalCandidates() async throws -> [ChatRequestRenewalCandidate]

    func updateAnonymousPeriod(requestId: String, period: UInt32) async throws
}

final class ChatRequestDeliveryStore {
    private let databaseService: CoreDataServiceProtocol

    init(storageFacade: StorageFacadeProtocol) {
        databaseService = storageFacade.databaseService
    }
}

extension ChatRequestDeliveryStore: ChatRequestDeliveryStoring {
    func isAwaitingDelivery(requestId: String) async throws -> Bool {
        try await databaseService.performRead { context in
            guard let request = try Self.fetchRequest(requestId, in: context) else { return false }

            return request.status == Chat.RequestStatus.outgoing.rawValue &&
                request.message?.status == Chat.LocalMessage.Status.outgoing(.new).rawValue &&
                request.contact?.chatRequest == request
        }
    }

    func markDelivered(requestId: String, anonymousPeriod: UInt32) async throws {
        try await databaseService.performWrite { context in
            guard let request = try Self.fetchRequest(requestId, in: context) else { return }

            request.anonymousDeliveryPeriod = NSNumber(value: Int32(bitPattern: anonymousPeriod))
            request.message?.status = Chat.LocalMessage.Status.outgoing(.sent).rawValue
            request.message?.touchParent()
            request.touchParent()
        }
    }

    func markFailed(requestId: String) async throws {
        try await databaseService.performWrite { context in
            guard let request = try Self.fetchRequest(requestId, in: context) else { return }

            request.message?.status = Chat.LocalMessage.Status.outgoing(.failed).rawValue
            request.message?.touchParent()
            request.touchParent()
        }
    }
}

extension ChatRequestDeliveryStore: ChatRequestRenewalStoring {
    func renewalCandidates() async throws -> [ChatRequestRenewalCandidate] {
        try await databaseService.performRead { context in
            let fetchRequest = CDChatContact.fetchRequest()
            fetchRequest.predicate = Self.renewablePredicate()
            fetchRequest.sortDescriptors = [
                NSSortDescriptor(key: #keyPath(CDChatContact.chatRequest.timestamp), ascending: false)
            ]

            let mapper = ChatContactMapper()

            return try context.fetch(fetchRequest)
                .map { try mapper.transform(entity: $0) }
                .compactMap(Self.renewalCandidate(from:))
        }
    }

    func updateAnonymousPeriod(requestId: String, period: UInt32) async throws {
        try await databaseService.performWrite { context in
            guard let request = try Self.fetchRequest(requestId, in: context) else { return }

            request.anonymousDeliveryPeriod = NSNumber(value: Int32(bitPattern: period))
        }
    }
}

private extension ChatRequestDeliveryStore {
    static func renewablePredicate() -> NSPredicate {
        let sentStatuses = [
            Chat.LocalMessage.Status.outgoing(.sent).rawValue,
            Chat.LocalMessage.Status.outgoing(.delivered).rawValue
        ]

        return NSPredicate(
            format: "%K == %d AND %K != nil AND %K IN %@",
            #keyPath(CDChatContact.chatRequest.status),
            Chat.RequestStatus.outgoing.rawValue,
            #keyPath(CDChatContact.chatRequest.anonymousDeliveryPeriod),
            #keyPath(CDChatContact.chatRequest.message.status),
            sentStatuses
        )
    }

    static func renewalCandidate(from contact: Chat.Contact) -> ChatRequestRenewalCandidate? {
        guard
            let request = contact.chatRequest,
            let period = request.anonymousDeliveryPeriod,
            let localMessage = request.message,
            let message = Chat.RequestMessage(localMessage: localMessage)
        else {
            return nil
        }

        return ChatRequestRenewalCandidate(
            message: message,
            session: contact.toMessageExchangeSessionRequest(),
            period: period
        )
    }

    static func fetchRequest(_ requestId: String, in context: NSManagedObjectContext) throws -> CDChatRequest? {
        let fetchRequest = CDChatRequest.fetchRequest()
        fetchRequest.predicate = .chatRequestById(requestId)
        fetchRequest.fetchLimit = 1

        return try context.fetch(fetchRequest).first
    }
}
