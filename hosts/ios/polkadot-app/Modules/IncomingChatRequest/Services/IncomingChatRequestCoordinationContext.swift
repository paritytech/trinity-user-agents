import Foundation
import SubstrateSdk
import Operation_iOS
import MessageExchangeKit

actor IncomingChatRequestCoordinationContext {
    let remoteContactResolver: RemoteContactResolving
    let requestStoreService: ChatRequestStoreServicing
    let messageExchangeModeProvider: any MessageExchangeModeProviding
    private let contactsStorageService: ContactsLocalStorageServicing

    let discoveryOwnKeyIds: Set<Chat.Contact.Own>
    let matchOwnKeyIds: Set<Chat.Contact.Own>

    private var contactsById: [AccountId: Chat.Contact] = [:]
    private var discoveryTasks: [Chat.Contact.Own: Task<Void, Never>] = [:]
    private var incomingRequestTasks: [Chat.Contact.Own: Task<Void, Never>] = [:]

    init(
        discoveryOwnKeyIds: Set<Chat.Contact.Own>,
        matchOwnKeyIds: Set<Chat.Contact.Own>,
        requestStoreService: ChatRequestStoreServicing,
        messageExchangeModeProvider: any MessageExchangeModeProviding,
        contactsStorageService: ContactsLocalStorageServicing,
        remoteContactResolver: RemoteContactResolving
    ) {
        self.discoveryOwnKeyIds = discoveryOwnKeyIds
        self.matchOwnKeyIds = matchOwnKeyIds
        self.remoteContactResolver = remoteContactResolver
        self.requestStoreService = requestStoreService
        self.messageExchangeModeProvider = messageExchangeModeProvider
        self.contactsStorageService = contactsStorageService
    }

    func update(
        contacts: [Chat.Contact],
        discoverTaskBuilder: (Chat.Contact.Own) -> Task<Void, Never>,
        incomingRequestTaskBuilder: ([Chat.Contact], Chat.Contact.Own) -> Task<Void, Never>
    ) {
        let newContactsWithOutgoingRequests = contacts
            .filter(\.hasOutgoingChatRequest)
            .buildOwnKeyIdMap()

        let prevContactsWithOutgoingRequests = contactsById.values
            .filter(\.hasOutgoingChatRequest)
            .buildOwnKeyIdMap()

        contactsById = contacts.reduce(into: [:]) { $0[$1.accountId] = $1 }

        for ownKeyId in discoveryOwnKeyIds where discoveryTasks[ownKeyId] == nil {
            discoveryTasks[ownKeyId] = discoverTaskBuilder(ownKeyId)
        }

        for ownKeyId in matchOwnKeyIds {
            let prevContactsForOwnId = prevContactsWithOutgoingRequests[ownKeyId] ?? [:]

            let newContactsForOwnId = newContactsWithOutgoingRequests[ownKeyId] ?? [:]

            if Set(prevContactsForOwnId.keys) != Set(newContactsForOwnId.keys) {
                incomingRequestTasks[ownKeyId]?.cancel()
                incomingRequestTasks[ownKeyId] = nil

                if !newContactsForOwnId.isEmpty {
                    let targetContacts = Array(newContactsForOwnId.values)
                    incomingRequestTasks[ownKeyId] = incomingRequestTaskBuilder(targetContacts, ownKeyId)
                }
            }
        }
    }

    func handle(
        incomingRequest: ChatRequest.ValidatedRemoteModel,
        ownKeyId: Chat.Contact.Own
    ) async throws {
        if let contact = contactsById[incomingRequest.peerAccountId] {
            guard contact.ownKeyId == ownKeyId else {
                throw HandleError.ownKeyIdNotMatch(contact.username)
            }

            try await handleForExistingContact(contact, incomingRequest: incomingRequest)
        } else {
            try await handleNew(incomingRequest: incomingRequest, ownKeyId: ownKeyId)
        }
    }
}

extension IncomingChatRequestCoordinationContext {
    enum HandleError: Error {
        case noRemoteContact(AccountId)
        case ownKeyIdNotMatch(String)
    }
}

private extension IncomingChatRequestCoordinationContext {
    func handleNew(
        incomingRequest: ChatRequest.ValidatedRemoteModel,
        ownKeyId: Chat.Contact.Own
    ) async throws {
        // it might be some old request that still persisted even if contact is removed
        guard try await ensureNoLocalRequest(incomingRequest) else {
            return
        }

        let optContact = try await remoteContactResolver.fetch(by: incomingRequest.peerAccountId)

        guard let remoteContact = optContact else {
            throw HandleError.noRemoteContact(incomingRequest.peerAccountId)
        }

        try await requestStoreService.newIncomingRequestFromRemote(
            incomingRequest,
            contact: remoteContact,
            ownKeyId: ownKeyId
        )
    }

    func handleForExistingContact(
        _ contact: Chat.Contact,
        incomingRequest: ChatRequest.ValidatedRemoteModel
    ) async throws {
        guard let existingRequest = contact.chatRequest else {
            try await handleWhenContactWithoutRequests(
                remoteRequest: incomingRequest,
                contact: contact
            )

            return
        }

        switch existingRequest.status {
        case .incoming:
            try await handleWhenExistingIncomingRequest(
                existingRequest,
                remoteRequest: incomingRequest,
                contact: contact
            )
        case .outgoing:
            try await handleWhenExistingOutgoingRequest(
                existingRequest,
                remoteRequest: incomingRequest,
                contact: contact
            )
        }
    }

    func handleWhenExistingOutgoingRequest(
        _ existingOutgoingRequest: Chat.Request,
        remoteRequest: ChatRequest.ValidatedRemoteModel,
        contact: Chat.Contact
    ) async throws {
        // make sure no request exists
        guard try await ensureNoLocalRequest(remoteRequest) else {
            return
        }

        let messageExchangeMode = messageExchangeModeProvider.mode(for: contact.ownKeyId)
        let acceptorDevice = try requestStoreService.buildLocalAcceptorDevice(for: contact.ownKeyId)
        try await requestStoreService.acceptOutgoingRequest(
            existingOutgoingRequest.requestId,
            messageExchangeMode: messageExchangeMode,
            remoteIncoming: remoteRequest,
            acceptorDevice: acceptorDevice
        )

        await erasePushToken(contact)
        await eraseVoIPPushToken(contact)
    }

    func handleWhenExistingIncomingRequest(
        _ existingIncomingRequest: Chat.Request,
        remoteRequest: ChatRequest.ValidatedRemoteModel,
        contact: Chat.Contact
    ) async throws {
        if remoteRequest.requestId == existingIncomingRequest.requestId {
            // peer updated existing request
            guard remoteRequest.message.timestamp > existingIncomingRequest.timestamp else {
                return
            }

            try await requestStoreService.updateRequestFromRemote(remoteRequest)
        } else {
            // peer replaced previous request
            guard remoteRequest.message.timestamp > existingIncomingRequest.timestamp else {
                return
            }

            // make sure no request exists
            guard try await ensureNoLocalRequest(remoteRequest) else {
                return
            }

            try await requestStoreService.replaceRequestFromRemote(remoteRequest)
            await erasePushToken(contact)
            await eraseVoIPPushToken(contact)
        }
    }

    func handleWhenContactWithoutRequests(
        remoteRequest: ChatRequest.ValidatedRemoteModel,
        contact: Chat.Contact
    ) async throws {
        guard try await ensureNoLocalRequest(remoteRequest) else {
            return
        }

        let messageExchangeMode = messageExchangeModeProvider.mode(for: contact.ownKeyId)
        let acceptorDevice = try requestStoreService.buildLocalAcceptorDevice(for: contact.ownKeyId)
        try await requestStoreService.acceptIncomingRequest(
            .new(
                remoteRequest,
                messageExchangeMode: messageExchangeMode,
                acceptorDevice: acceptorDevice
            )
        )
        await erasePushToken(contact)
        await eraseVoIPPushToken(contact)
    }

    func ensureNoLocalRequest(_ remoteRequest: ChatRequest.ValidatedRemoteModel) async throws -> Bool {
        let localRequest = try await requestStoreService.fetchByRequestId(remoteRequest.requestId)

        return localRequest == nil
    }

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

private extension [Chat.Contact] {
    func buildOwnKeyIdMap() -> [Chat.Contact.Own: [AccountId: Chat.Contact]] {
        reduce(into: [:]) { accum, contact in
            var existingKeyId = accum[contact.ownKeyId] ?? [:]
            existingKeyId[contact.accountId] = contact
            accum[contact.ownKeyId] = existingKeyId
        }
    }
}
