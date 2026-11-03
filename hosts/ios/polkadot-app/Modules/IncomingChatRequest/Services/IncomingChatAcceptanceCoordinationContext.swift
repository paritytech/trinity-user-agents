import Foundation
import MessageExchangeKit
import SubstrateSdk

/// Listens for acceptances of our pending outgoing requests and applies each once; mirrors
/// `IncomingChatRequestCoordinationContext`.
actor IncomingChatAcceptanceCoordinationContext {
    private let requestStoreService: ChatRequestStoreServicing
    private let contactsStorageService: ContactsLocalStorageServicing

    let logger: LoggerProtocol

    private var processedRequestIds: Set<String> = []
    private var peersWithOutgoingRequests: [AccountId: MessageExchange.SessionRequest] = [:]
    private var incomingAcceptancesTask: Task<Void, Never>?

    init(
        requestStoreService: ChatRequestStoreServicing,
        contactsStorageService: ContactsLocalStorageServicing,
        logger: LoggerProtocol
    ) {
        self.requestStoreService = requestStoreService
        self.contactsStorageService = contactsStorageService
        self.logger = logger
    }
}

extension IncomingChatAcceptanceCoordinationContext {
    func update(
        contacts: [Chat.Contact],
        incomingAcceptanceTaskBuilder: ([MessageExchange.SessionRequest]) -> Task<Void, Never>
    ) {
        let newPeers: [AccountId: MessageExchange.SessionRequest] = contacts.reduce(into: [:]) { peers, contact in
            guard contact.hasOutgoingChatRequest else { return }

            peers[contact.accountId] = MessageExchange.SessionRequest(
                own: contact.ownKeyId.toMessageExchangeOwn(),
                peer: contact.toMessageExchangePeer()
            )
        }

        guard newPeers != peersWithOutgoingRequests else { return }

        peersWithOutgoingRequests = newPeers

        incomingAcceptancesTask?.cancel()
        incomingAcceptancesTask = newPeers.isEmpty ? nil : incomingAcceptanceTaskBuilder(Array(newPeers.values))
    }

    func cancelAll() {
        incomingAcceptancesTask?.cancel()
        incomingAcceptancesTask = nil
        peersWithOutgoingRequests = [:]
    }

    /// Copies from earlier periods can coexist; each request is accepted once.
    @discardableResult
    func handle(_ acceptance: IncomingChatAcceptanceInput) async throws -> Chat.Request? {
        let requestId = acceptance.requestId
        let contactId = acceptance.peerAccountId
        let peerDevice = acceptance.device

        guard !processedRequestIds.contains(requestId) else {
            return nil
        }

        processedRequestIds.insert(requestId)

        guard let request = try await requestStoreService.fetchByRequestId(requestId) else {
            logger.debug("No request found")
            return nil
        }

        guard request.contactAccountId == contactId else {
            logger.error("Handling request from another contact")
            return nil
        }

        guard request.isOutgoing else {
            logger.error("Can't accept incoming request")
            return nil
        }

        do {
            try await requestStoreService.acceptOutgoingRequest(
                requestId,
                remoteIncoming: nil,
                acceptorDevice: nil,
                peerDevice: peerDevice
            )
        } catch {
            // Let the next copy of the acceptance retry the save.
            processedRequestIds.remove(requestId)
            throw error
        }

        logger.debug("Request has been accepted")

        guard let contact = try await contactsStorageService
            .getContact(by: contactId)
            .asyncExecute()
        else {
            return request
        }

        await erasePushToken(contact)
        await eraseVoIPPushToken(contact)

        return request
    }
}

private extension IncomingChatAcceptanceCoordinationContext {
    /// Remove own push token for an existing contact after accepting,
    /// it will be updated in ChatPushService later
    func erasePushToken(_ contact: Chat.Contact) async {
        let settings = Chat.ContactOutgoingSettings(contact: contact)
        guard settings.ownPushToken != nil else {
            return
        }
        let newSettings = settings.updatingOwnPushToken(nil)
        try? await contactsStorageService.updateOugoingSettings([newSettings]).asyncExecute()
    }

    /// Remove own voip push token for an existing contact after accepting,
    /// it will be updated in ChatPushService later
    func eraseVoIPPushToken(_ contact: Chat.Contact) async {
        let settings = Chat.VoIPContactOutgoingSettings(contact: contact)
        guard settings.voipOwnPushToken != nil else {
            return
        }
        let newSettings = settings.updatingVoIPOwnPushToken(nil)
        try? await contactsStorageService.updateVoIPOugoingSettings([newSettings]).asyncExecute()
    }
}
