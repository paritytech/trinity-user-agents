import CoreData
import Foundation
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

struct DeviceSyncChatAcceptedApplier {
    private let storageFacade: StorageFacadeProtocol
    private let contactsStorageService: ContactsLocalStorageServicing
    private let peerLogger: LoggerProtocol

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        contactsStorageService: ContactsLocalStorageServicing = ContactsLocalStorageService(),
        peerLogger: LoggerProtocol
    ) {
        self.storageFacade = storageFacade
        self.contactsStorageService = contactsStorageService
        self.peerLogger = peerLogger
    }

    func apply(_ wireMessages: [Chat.DeviceSyncWireMessage]) async throws {
        var processedRequestIds = Set<String>()
        let sortedMessages = wireMessages.sorted { $0.remote.timestamp < $1.remote.timestamp }

        for wireMessage in sortedMessages {
            guard let accepted = chatAccepted(from: wireMessage.remote) else {
                continue
            }

            guard processedRequestIds.insert(accepted.requestId).inserted else {
                continue
            }

            try await applyAcceptedRequestIfNeeded(
                accepted.requestId,
                from: wireMessage.peerId,
                status: wireMessage.status,
                peerDevice: accepted.peerDevice
            )
        }
    }
}

private extension DeviceSyncChatAcceptedApplier {
    func applyAcceptedRequestIfNeeded(
        _ requestId: String,
        from contactId: AccountId,
        status: Chat.DeviceSyncLocalStatus,
        peerDevice: Chat.PeerDevice?
    ) async throws {
        let requestRepository = storageFacade.createRepository(
            mapper: AnyCoreDataMapper(ChatRequestMapper())
        )

        guard let request = try await requestRepository
            .fetchOperation(by: { requestId }, options: .init())
            .asyncExecute()
        else {
            peerLogger.debug("No synced accepted request found with id: \(requestId)")
            return
        }

        guard request.contactAccountId == contactId else {
            peerLogger.error("Synced accepted request \(requestId) belongs to another contact")
            return
        }

        switch status {
        case .outgoing:
            try await applyOwnDeviceAcceptedRequest(request, requestId: requestId)
        case .incoming:
            try await applyPeerAcceptedRequest(
                request,
                requestId: requestId,
                contactId: contactId,
                peerDevice: peerDevice
            )
        }
    }

    func applyOwnDeviceAcceptedRequest(
        _ request: Chat.Request,
        requestId: String
    ) async throws {
        guard request.isIncoming else {
            peerLogger.debug("Synced outgoing accept \(requestId) doesn't match incoming request, skipping")
            return
        }

        try await markLocalIncomingRequestAccepted(requestId)
        peerLogger.debug("Accepted incoming request \(requestId) from sync")
    }

    func applyPeerAcceptedRequest(
        _ request: Chat.Request,
        requestId: String,
        contactId: AccountId,
        peerDevice: Chat.PeerDevice?
    ) async throws {
        guard request.isOutgoing else {
            peerLogger.debug("Synced incoming accept \(requestId) doesn't match outgoing request, skipping")
            return
        }

        guard try await contactsStorageService.getContact(by: contactId).asyncExecute() != nil else {
            peerLogger.error("Missing contact for synced accepted request")
            return
        }

        try await markLocalOutgoingRequestAccepted(requestId, peerDevice: peerDevice)

        peerLogger.debug("Accepted outgoing request \(requestId) from sync")
    }

    func markLocalIncomingRequestAccepted(_ requestId: String) async throws {
        let acceptIncoming = ChatRequest.SyncAcceptIncoming(requestId: requestId)

        try await storageFacade.createRepository(
            mapper: AnyCoreDataMapper(SyncAcceptIncomingChatRequestMapper())
        )
        .saveOperation({ [acceptIncoming] }, { [] })
        .asyncExecute()
    }

    func markLocalOutgoingRequestAccepted(_ requestId: String, peerDevice: Chat.PeerDevice?) async throws {
        let acceptOutgoing = ChatRequest.AcceptOutgoing(
            requestId: requestId,
            incomingRequest: nil,
            acceptorDevice: nil,
            peerDevice: peerDevice
        )

        try await storageFacade.createRepository(
            mapper: AnyCoreDataMapper(AcceptOutgoingChatRequestMapper())
        )
        .saveOperation({ [acceptOutgoing] }, { [] })
        .asyncExecute()
    }

    func chatAccepted(
        from message: Chat.RemoteMessage
    ) -> (requestId: String, peerDevice: Chat.PeerDevice?)? {
        guard let content = message.versioned.ensureV1()?.content else {
            return nil
        }

        switch content {
        case let .chatAccepted(model):
            return (model.messageId, nil)
        case let .multiChatAccepted(model):
            return (model.requestId, model.device)
        default:
            return nil
        }
    }
}

private extension ChatRequest {
    struct SyncAcceptIncoming {
        let requestId: String
    }
}

extension ChatRequest.SyncAcceptIncoming: Identifiable {
    var identifier: String {
        requestId
    }
}

private final class SyncAcceptIncomingChatRequestMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = ChatRequest.SyncAcceptIncoming
    typealias CoreDataEntity = CDChatRequest
}

extension SyncAcceptIncomingChatRequestMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case noExistingRequest
        case notIncomingRequest
    }

    func transform(entity _: CoreDataEntity) throws -> DataProviderModel {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CoreDataEntity,
        from _: DataProviderModel,
        using _: NSManagedObjectContext
    ) throws {
        guard entity.identifier != nil else {
            throw MappingError.noExistingRequest
        }

        guard let status = Chat.RequestStatus(rawValue: entity.status), status.isIncoming else {
            throw MappingError.notIncomingRequest
        }

        entity.status = Chat.RequestStatus.incoming(.accepted).rawValue
        entity.touchParent()
        entity.contact?.acceptedAt = Date()
        entity.contact?.pendingDevicesFanOut = true
        entity.contact = nil
    }
}

private extension Chat.RequestStatus {
    var isIncoming: Bool {
        switch self {
        case .incoming:
            true
        case .outgoing:
            false
        }
    }
}
