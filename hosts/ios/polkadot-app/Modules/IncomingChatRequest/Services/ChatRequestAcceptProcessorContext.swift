import Foundation
import MessageExchangeKit
import SubstrateSdk

actor ChatRequestAcceptProcessorContext {
    private let requestStoreService: ChatRequestStoreServicing
    private let messageStoreService: MessagesLocalStorageServicing
    private let contactsStorageService: ContactsLocalStorageServicing

    let logger: LoggerProtocol

    private var processedRequestIds: Set<String> = []

    init(
        requestStoreService: ChatRequestStoreServicing,
        messageStoreService: MessagesLocalStorageServicing,
        contactsStorageService: ContactsLocalStorageServicing,
        logger: LoggerProtocol
    ) {
        self.requestStoreService = requestStoreService
        self.messageStoreService = messageStoreService
        self.contactsStorageService = contactsStorageService
        self.logger = logger
    }
}

extension ChatRequestAcceptProcessorContext {
    @discardableResult
    func accept(
        requestId: String,
        from contactId: AccountId,
        messageExchangeMode: MessageExchangeMode
    ) async throws -> Chat.Request? {
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

        try await requestStoreService.acceptOutgoingRequest(
            requestId,
            messageExchangeMode: messageExchangeMode,
            remoteIncoming: nil,
            acceptorDevice: nil
        )
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

    func markMessageAsDelivered(for requestId: String) async throws {
        try await messageStoreService.markAsDelivered([requestId]).asyncExecute()
    }

    func storePeerDevices(_ devices: [Chat.PeerDevice], for contactId: AccountId) async {
        let changes = devices.map { Chat.DeviceChange.added($0) }
        let settings = Chat.ContactDeviceSettings(
            accountId: contactId,
            changes: changes
        )
        do {
            try await contactsStorageService.updateDeviceSettings([settings]).asyncExecute()
        } catch {
            logger.error("Failed to store peer devices: \(error)")
        }
    }
}

private extension ChatRequestAcceptProcessorContext {
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
