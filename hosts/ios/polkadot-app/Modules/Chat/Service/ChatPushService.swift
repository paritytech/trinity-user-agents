import Foundation
import SubstrateSdk
import Operation_iOS
import MessageExchangeKit

protocol ChatPushServicing: AnyObject {
    func setContacts(_ contacts: [AccountId: Chat.Contact])
}

final class ChatPushService {
    private let apnsTokenObserver: any APNSTokenManaging
    private let pushKitService: PushKitServicing
    private let contactsStorageService: ContactsLocalStorageServicing
    private let pushIdFactory: ChatPushIdMaking
    private let workQueue: DispatchQueue
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol

    private var contacts = [AccountId: Chat.Contact]()
    private var pushKitTokenSubscription: Task<Void, Error>?

    init(
        apnsTokenObserver: any APNSTokenManaging,
        pushKitService: PushKitServicing = PushKitService.shared,
        contactsStorageService: ContactsLocalStorageServicing,
        pushIdFactory: ChatPushIdMaking,
        workQueue: DispatchQueue,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.apnsTokenObserver = apnsTokenObserver
        self.pushKitService = pushKitService
        self.contactsStorageService = contactsStorageService
        self.pushIdFactory = pushIdFactory
        self.workQueue = workQueue
        self.operationQueue = operationQueue
        self.logger = logger
        subscribeToOwnToken()
        subscribeToVoIPOwnToken()
    }

    deinit {
        unsubscribeFromOwnToken()
        unsubscribeFromVoIPOwnToken()
    }
}

// MARK: - IncomingMessageProcessing

extension ChatPushService: IncomingMessageProcessing {
    func process(messages: [Chat.RemoteMessage], from contact: Chat.Contact) {
        updatePushToken(from: messages, for: contact)
        updateVoIPPushToken(from: messages, for: contact)
    }
}

// MARK: - ChatPushServicing

extension ChatPushService: ChatPushServicing {
    func setContacts(_ contacts: [AccountId: Chat.Contact]) {
        dispatchPrecondition(condition: .onQueue(workQueue))
        self.contacts = contacts

        updateOutgoingSettings()

        Task {
            guard let token = await pushKitService.token(with: .voIP) else {
                return
            }
            updateVoIPOutgoingSettings(for: .init(type: .voIP, payload: token))
        }
    }
}

// MARK: - Push token

private extension ChatPushService {
    func updatePushToken(
        from messages: [Chat.RemoteMessage],
        for contact: Chat.Contact
    ) {
        guard let tokenContent = remoteTokenContent(from: messages, isVoIP: false) else {
            return
        }

        logger.debug("Updating push token content for \(contact.username)")

        let operation = contactsStorageService.updateIncomingSettings(
            [
                Chat.ContactIncomingSettings(
                    accountId: contact.accountId,
                    pushToken: tokenContent.token,
                    peerPlatform: tokenContent.pushType.platform
                )
            ]
        )

        execute(
            operation: operation,
            inOperationQueue: operationQueue,
            runningCallbackIn: workQueue
        ) { [weak self] result in
            switch result {
            case .success:
                self?.logger.info("Push token updated successfully for \(contact.username)")
            case let .failure(error):
                self?.logger.warning("Push token save failed for \(contact.username): \(error)")
            }
        }
    }

    func updateVoIPPushToken(
        from messages: [Chat.RemoteMessage],
        for contact: Chat.Contact
    ) {
        guard let tokenContent = remoteTokenContent(from: messages, isVoIP: true) else {
            return
        }

        logger.debug("Updating VoIP push token content for \(contact.username)")

        let operation = contactsStorageService.updateVoIPIncomingSettings(
            [
                Chat.VoIPContactIncomingSettings(
                    accountId: contact.accountId,
                    voipPushToken: tokenContent.token
                )
            ]
        )

        execute(
            operation: operation,
            inOperationQueue: operationQueue,
            runningCallbackIn: workQueue
        ) { [weak self] result in
            switch result {
            case .success:
                self?.logger.info("VoIP push token updated successfully for \(contact.username)")
            case let .failure(error):
                self?.logger.warning("VoIP push token save failed for \(contact.username): \(error)")
            }
        }
    }

    func remoteTokenContent(
        from messages: [Chat.RemoteMessage],
        isVoIP: Bool
    ) -> Chat.RemoteTokenContent? {
        var latestTokenMessage: Chat.RemoteMessage?
        var latestTokenContent: Chat.RemoteTokenContent?

        for message in messages {
            guard
                case let .token(tokenContent) = message.versioned.ensureV1()?.content,
                isVoIP == tokenContent.pushType.isVoIP
            else {
                continue
            }

            if let tokenMessage = latestTokenMessage {
                if message.timestamp > tokenMessage.timestamp {
                    latestTokenMessage = message
                    latestTokenContent = tokenContent
                }
            } else {
                latestTokenMessage = message
                latestTokenContent = tokenContent
            }
        }

        return latestTokenContent
    }
}

// MARK: - Last own token

private extension ChatPushService {
    func subscribeToOwnToken() {
        apnsTokenObserver.add(
            observer: self,
            sendStateOnSubscription: true,
            queue: workQueue
        ) { [weak self] _, _ in
            self?.updateOutgoingSettings()
        }
    }

    func unsubscribeFromOwnToken() {
        apnsTokenObserver.remove(observer: self)
    }

    func updateOutgoingSettings() {
        logger.debug("Checking for outgoing settings update")

        var changedSettings: [Chat.ContactOutgoingSettings] = []

        for contact in contacts.values {
            let originalSettings = Chat.ContactOutgoingSettings(contact: contact)

            var newSettings = originalSettings

            if let lastOwnToken = apnsTokenObserver.currentToken, lastOwnToken != newSettings.ownPushToken {
                logger.debug("Last own token changes for \(contact.username)")
                newSettings = newSettings.updatingOwnPushToken(lastOwnToken)
            }

            if let pushId = updatedPeerPushId(for: contact, currentPeerPushId: newSettings.peerPushId) {
                logger.debug("Push id changes for \(contact.username)")
                newSettings = newSettings.updatingPeerPushId(pushId)
            }

            if newSettings != originalSettings {
                changedSettings.append(newSettings)
            }
        }

        guard !changedSettings.isEmpty else {
            logger.debug("No changes in outgoing settings")
            return
        }

        logger.debug("Updating outgoing settings: \(changedSettings.count)")

        let operation = contactsStorageService.updateOugoingSettings(changedSettings)

        execute(
            operation: operation,
            inOperationQueue: operationQueue,
            runningCallbackIn: workQueue
        ) { [logger] result in
            switch result {
            case .success:
                logger.info("Outgoing settings updated")
            case let .failure(error):
                logger.warning("Outgoing settings update failed: \(error)")
            }
        }
    }
}

// MARK: - VoIP last own token

private extension ChatPushService {
    func subscribeToVoIPOwnToken() {
        pushKitTokenSubscription = Task { [weak self] in
            do {
                guard let sequence = self?.pushKitService.observeToken() else {
                    return
                }
                for try await token in sequence {
                    self?.updateVoIPOutgoingSettings(for: token)
                }
            } catch {
                self?.logger.error("Error: \(error)")
            }
        }
    }

    func unsubscribeFromVoIPOwnToken() {
        pushKitTokenSubscription?.cancel()
        pushKitTokenSubscription = nil
    }

    func updateVoIPOutgoingSettings(for token: PushKitToken) {
        logger.debug("Checking for voip outgoing settings update")

        var changedSettings: [Chat.VoIPContactOutgoingSettings] = []

        for contact in contacts.values {
            let originalSettings = Chat.VoIPContactOutgoingSettings(contact: contact)

            var newSettings = originalSettings
            let voipLastOwnToken = token.payload

            if voipLastOwnToken != newSettings.voipOwnPushToken {
                logger.debug("VoIP last own token changes for \(contact.username)")
                newSettings = newSettings.updatingVoIPOwnPushToken(voipLastOwnToken)
            }

            if let pushId = updatedPeerPushId(for: contact, currentPeerPushId: newSettings.peerPushId) {
                logger.debug("Push id changes for \(contact.username)")
                newSettings = newSettings.updatingPeerPushId(pushId)
            }

            if newSettings != originalSettings {
                changedSettings.append(newSettings)
            }
        }

        guard !changedSettings.isEmpty else {
            logger.debug("No changes in VoIP outgoing settings")
            return
        }

        logger.debug("Updating VoIP outgoing settings: \(changedSettings.count)")

        let operation = contactsStorageService.updateVoIPOugoingSettings(changedSettings)

        execute(
            operation: operation,
            inOperationQueue: operationQueue,
            runningCallbackIn: workQueue
        ) { [logger] result in
            switch result {
            case .success:
                logger.info("VoIP outgoing settings updated")
            case let .failure(error):
                logger.warning("VoIP outgoing settings update failed: \(error)")
            }
        }
    }

    func updatedPeerPushId(for contact: Chat.Contact, currentPeerPushId: String?) -> String? {
        guard
            let pushId = pushIdFactory.makePushId(
                peer: contact.toMessageExchangePeer(),
                own: contact.ownKeyId.toMessageExchangeOwn()
            ),
            pushId.peerString != currentPeerPushId
        else {
            return nil
        }
        return pushId.peerString
    }
}
