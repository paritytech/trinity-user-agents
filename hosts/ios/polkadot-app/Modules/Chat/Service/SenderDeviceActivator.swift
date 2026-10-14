import Foundation
import SubstrateSdk
import MessageExchangeKit

/// Stores the sender's device on the contact after we accept a V2 chat request.
/// This transitions the session from identity-level to device-level.
final class SenderDeviceActivator {
    private let contactsStorageService: ContactsLocalStorageServicing
    private let chatRequestStoreService: ChatRequestStoreServicing
    private let messageExchangeModeProvider: MessageExchangeModeProviding
    private let logger: LoggerProtocol

    init(
        contactsStorageService: ContactsLocalStorageServicing,
        chatRequestStoreService: ChatRequestStoreServicing,
        messageExchangeModeProvider: MessageExchangeModeProviding,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.contactsStorageService = contactsStorageService
        self.chatRequestStoreService = chatRequestStoreService
        self.messageExchangeModeProvider = messageExchangeModeProvider
        self.logger = logger
    }

    func handleSentMessages(
        _ messages: [Chat.RemoteMessage],
        to peer: MessageExchange.Peer,
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        guard error == nil else { return }

        let acceptedRequestIds = messages.compactMap { message -> String? in
            if case let .multiChatAccepted(content) = message.versioned.ensureV1()?.content {
                return content.requestId
            }
            return nil
        }

        guard !acceptedRequestIds.isEmpty else { return }

        Task { [weak self] in
            guard let self else { return }

            do {
                guard let contact = try await contactsStorageService
                    .getContact(by: peer.accountId)
                    .asyncExecute()
                else {
                    logger.error("Missing contact for sender device activation")
                    return
                }

                guard messageExchangeModeProvider.mode(for: contact) == .multidevice else {
                    return
                }

                var changes = [Chat.DeviceChange]()
                for requestId in acceptedRequestIds {
                    let senderDevice = try await fetchSenderDevice(for: requestId)
                    changes.append(.added(senderDevice))
                }
                let settings = Chat.ContactDeviceSettings(accountId: peer.accountId, changes: changes)
                try await contactsStorageService.updateDeviceSettings([settings]).asyncExecute()
            } catch {
                logger.error("Failed to activate sender devices: \(error) \(peer.accountId.toHex())")
            }
        }
    }
}

private extension SenderDeviceActivator {
    func fetchSenderDevice(for requestId: String) async throws -> Chat.PeerDevice {
        guard let senderDevice = try await chatRequestStoreService.fetchPeerDeviceByRequestId(requestId) else {
            throw SenderDeviceActivatorError.noSenderDevice
        }
        return senderDevice
    }
}

enum SenderDeviceActivatorError: Error {
    case noSenderDevice
}
