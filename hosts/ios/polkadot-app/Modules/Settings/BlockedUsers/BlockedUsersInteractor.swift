import Foundation
import Operation_iOS
import Foundation_iOS
import SubstrateSdk

final class BlockedUsersInteractor {
    weak var presenter: BlockedUsersInteractorOutputProtocol?

    private let chatContactDataProviderFactory: ChatContactDataProviderMaking
    private let blockUserService: BlockUserServicing
    private let logger: LoggerProtocol

    private var subscriptionTask: Task<Void, Never>?

    init(
        chatContactDataProviderFactory: ChatContactDataProviderMaking,
        blockUserService: BlockUserServicing,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.chatContactDataProviderFactory = chatContactDataProviderFactory
        self.blockUserService = blockUserService
        self.logger = logger
    }

    deinit {
        subscriptionTask?.cancel()
    }
}

extension BlockedUsersInteractor: BlockedUsersInteractorInputProtocol {
    func setup() {
        subscriptionTask = Task { [weak self, chatContactDataProviderFactory, logger] in
            let stream = chatContactDataProviderFactory.subscribeBlockedContacts()

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
