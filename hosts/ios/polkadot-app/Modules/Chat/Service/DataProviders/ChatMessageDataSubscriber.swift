import Foundation
import Operation_iOS
import SubstrateSdk

protocol ChatMessageDataSubscribing: LocalStorageProviderObserving where Self: AnyObject {
    var chatMessageDataHandler: ChatMessageDataHandling { get }
    var chatMessageDataProviderFactory: ChatMessageDataProviderMaking { get }

    func subscribeOnNewMessagesLifecycle(
        on queue: DispatchQueue
    ) -> StreamableProvider<Chat.LocalMessage>
}

extension ChatMessageDataSubscribing where Self: ChatMessageDataHandling {
    var chatMessageDataHandler: ChatMessageDataHandling { self }

    func subscribeOnNewMessagesLifecycle(on queue: DispatchQueue) -> StreamableProvider<Chat.LocalMessage> {
        let provider = chatMessageDataProviderFactory.createNewRemoteMessagesLifecycleProvider()

        addStreamableProviderObserver(
            for: provider,
            updateClosure: { [weak self] changes in
                self?.chatMessageDataHandler.handleChatMessages(result: .success(changes))
            },
            failureClosure: { [weak self] error in
                self?.chatMessageDataHandler.handleChatMessages(result: .failure(error))
            },
            callbackQueue: queue,
            options: .allNonblocking()
        )

        return provider
    }
}
