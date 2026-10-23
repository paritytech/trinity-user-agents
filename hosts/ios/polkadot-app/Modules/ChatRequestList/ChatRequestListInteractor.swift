import UIKit
import Operation_iOS
import Foundation_iOS

final class ChatRequestListInteractor {
    weak var presenter: ChatRequestListInteractorOutputProtocol?

    let chatsProvider: ChatContactDataProviderMaking
    let logger: LoggerProtocol

    private var task: Task<Void, Never>?

    init(chatsProvider: ChatContactDataProviderMaking, logger: LoggerProtocol) {
        self.chatsProvider = chatsProvider
        self.logger = logger
    }

    deinit {
        task?.cancel()
    }
}

extension ChatRequestListInteractor: ChatRequestListInteractorInputProtocol {
    func setup() {
        task = Task { [weak self, chatsProvider, logger] in
            // The requests of the chat product are its rooms with an incoming contact (HX2).
            let chatsStream = chatsProvider.subscribeChatsWithPredicate(.listedChats())

            do {
                for try await chats in chatsStream {
                    logger.debug("Requests received: \(chats.count)")
                    await self?.presenter?.didReceiveChats(chats)
                }

                logger.debug("Chats stream completed")
            } catch {
                logger.debug("Unexpected error: \(error)")
            }
        }
    }
}
