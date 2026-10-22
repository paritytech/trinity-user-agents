import Foundation
import AsyncExtensions

final class ShareInteractor {
    weak var presenter: ShareInteractorOutputProtocol?

    private let chatContactDataProviderFactory: ChatContactDataProviderMaking
    private let messageSender: LocalMessageCreatingOperationMaking
    private let composer: ShareContentComposing
    private let productRooms: ShareProductRoomsProviding?
    private let logger: LoggerProtocol

    private var subscriptionTask: Task<Void, Never>?
    private var sendTask: Task<Void, Never>?

    init(
        chatContactDataProviderFactory: ChatContactDataProviderMaking,
        messageSender: LocalMessageCreatingOperationMaking,
        composer: ShareContentComposing,
        productRooms: ShareProductRoomsProviding? = nil,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chatContactDataProviderFactory = chatContactDataProviderFactory
        self.messageSender = messageSender
        self.composer = composer
        self.productRooms = productRooms
        self.logger = logger
    }

    deinit {
        subscriptionTask?.cancel()
        sendTask?.cancel()
    }
}

extension ShareInteractor: ShareInteractorInputProtocol {
    func setup() {
        subscriptionTask = Task { [weak self, chatContactDataProviderFactory, productRooms, logger] in
            do {
                let stream = chatContactDataProviderFactory.subscribeChatsWithPredicate(
                    .listedChats(productChat: PersonChatMode.isProductChat)
                )
                for try await chats in stream {
                    let targets = chats.compactMap { chat -> ChatWithPeerMetadata? in
                        switch chat.peer {
                        case let .person(contact):
                            guard contact.isReadyForMessaging else { return nil }
                            return ChatWithPeerMetadata(chat: chat, peerMetadata: contact.toPeerMetadata().list)
                        case .chatExtension:
                            return productRooms?.target(for: chat)
                        }
                    }
                    await self?.presenter?.didReceive(chats: targets)
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
        sendTask = Task { [weak self, messageSender, composer, productRooms, logger] in
            await self?.presenter?.didReceive(isLoading: true)

            let content = composer.compose(items: items, userMessage: userMessage)
            do {
                try await withThrowingTaskGroup(of: Void.self) { group in
                    for chatId in chatIds {
                        group.addTask {
                            if case .chatExtension = chatId, let productRooms, case let .text(text) = content {
                                try await productRooms.send(text: text, to: chatId)
                            } else {
                                try await messageSender.send(content: content, to: chatId)
                            }
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
