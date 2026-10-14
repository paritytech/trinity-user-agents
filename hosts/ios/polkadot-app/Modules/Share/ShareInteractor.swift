import Foundation
import AsyncExtensions

final class ShareInteractor {
    weak var presenter: ShareInteractorOutputProtocol?

    private let chatContactDataProviderFactory: ChatContactDataProviderMaking
    private let messageSender: LocalMessageCreatingOperationMaking
    private let composer: ShareContentComposing
    private let logger: LoggerProtocol

    private var subscriptionTask: Task<Void, Never>?
    private var sendTask: Task<Void, Never>?

    init(
        chatContactDataProviderFactory: ChatContactDataProviderMaking,
        messageSender: LocalMessageCreatingOperationMaking,
        composer: ShareContentComposing,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chatContactDataProviderFactory = chatContactDataProviderFactory
        self.messageSender = messageSender
        self.composer = composer
        self.logger = logger
    }

    deinit {
        subscriptionTask?.cancel()
        sendTask?.cancel()
    }
}

extension ShareInteractor: ShareInteractorInputProtocol {
    func setup() {
        subscriptionTask = Task { [weak self, chatContactDataProviderFactory, logger] in
            do {
                let stream = chatContactDataProviderFactory.subscribeChatsWithPredicate(
                    .chatWithNonBlockedContact()
                )
                for try await chats in stream {
                    let personChats = chats.compactMap { chat -> ChatWithPeerMetadata? in
                        guard
                            case let .person(contact) = chat.peer,
                            contact.isReadyForMessaging
                        else { return nil }
                        return ChatWithPeerMetadata(chat: chat, peerMetadata: contact.toPeerMetadata())
                    }
                    await self?.presenter?.didReceive(chats: personChats)
                }
            } catch {
                logger.error("Share chat subscription error: \(error)")
                await self?.presenter?.didReceive(error: error)
            }
        }
    }

    func send(items: [ShareItem], userMessage: String?, to chatIds: [Chat.Id]) {
        guard !chatIds.isEmpty else { return }

        sendTask?.cancel()
        sendTask = Task { [weak self, messageSender, composer, logger] in
            await self?.presenter?.didReceive(isLoading: true)

            let content = composer.compose(items: items, userMessage: userMessage)
            do {
                try await withThrowingTaskGroup(of: Void.self) { group in
                    for chatId in chatIds {
                        group.addTask {
                            try await messageSender.send(content: content, to: chatId)
                        }
                    }
                    try await group.waitForAll()
                }
                await self?.finishSend(error: nil)
            } catch {
                logger.error("Share send error: \(error)")
                await self?.finishSend(error: error)
            }
        }
    }
}

private extension ShareInteractor {
    func finishSend(error: Error?) async {
        await presenter?.didReceive(isLoading: false)
        if let error {
            await presenter?.didReceive(error: error)
        } else {
            await presenter?.didCompleteSend()
        }
    }
}
