import Foundation
import Operation_iOS
import Foundation_iOS
import SubstrateSdk
import AsyncExtensions

final class BlockedUsersInteractor {
    weak var presenter: BlockedUsersInteractorOutputProtocol?

    private let chatContactDataProviderFactory: ChatContactDataProviderMaking
    private let blockUserService: BlockUserServicing
    /// The blocked contacts are the blocked contacts of the chat product.
    private let productChat: Bool
    private let logger: LoggerProtocol

    private var subscriptionTask: Task<Void, Never>?

    init(
        chatContactDataProviderFactory: ChatContactDataProviderMaking,
        blockUserService: BlockUserServicing,
        productChat: Bool = false,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chatContactDataProviderFactory = chatContactDataProviderFactory
        self.blockUserService = blockUserService
        self.productChat = productChat
        self.logger = logger
    }

    deinit {
        subscriptionTask?.cancel()
    }
}

extension BlockedUsersInteractor: BlockedUsersInteractorInputProtocol {
    func setup() {
        subscriptionTask = Task { [weak self, chatContactDataProviderFactory, productChat, logger] in
            let stream = productChat
                ? chatContactDataProviderFactory.subscribeProductBlockedContacts()
                : chatContactDataProviderFactory.subscribeBlockedContacts()

            do {
                for try await contacts in stream {
                    let contactsById = Dictionary(
                        uniqueKeysWithValues: contacts.map { ($0.identifier, $0) }
                    )
                    await self?.presenter?.didReceive(contactsById: contactsById)
                }
            } catch {
                logger.error("Blocked contacts subscription error: \(error)")
            }
        }
    }

    func unblockUser(accountId: AccountId) {
        Task { [weak self] in
            do {
                try await self?.blockUserService.unblockUser(accountId: accountId)
            } catch {
                self?.logger.error("Failed to unblock user: \(error)")
            }
        }
    }
}

private extension ChatContactDataProviderMaking {
    /// The contacts that the user blocked in the chat product.
    func subscribeProductBlockedContacts() -> AnyAsyncSequence<[Chat.Contact]> {
        subscribeChatsWithPredicate(.listedChats(productChat: true))
            .map { chats in
                chats.compactMap(ProductContact.init(chat:)).filter(\.isBlocked).map(\.contact)
            }
            .eraseToAnyAsyncSequence()
    }
}
