import Foundation
import Operation_iOS

protocol ChatContactDataSubscribing: LocalStorageProviderObserving where Self: AnyObject {
    var chatContactDataHandler: ChatContactDataHandling { get }
    var chatContactDataProviderFactory: ChatContactDataProviderMaking { get }

    func subscribeOnChatContacts(on queue: DispatchQueue) -> StreamableProvider<Chat.Contact>
}

extension ChatContactDataSubscribing where Self: ChatContactDataHandling {
    var chatContactDataHandler: ChatContactDataHandling { self }

    func subscribeOnChatContacts(on queue: DispatchQueue) -> StreamableProvider<Chat.Contact> {
        let provider = chatContactDataProviderFactory.createAllContactsProvider()

        addStreamableProviderObserver(
            for: provider,
            updateClosure: { [weak self] changes in
                self?.chatContactDataHandler.handleChatContacts(
                    result: .success(changes)
                )
            },
            failureClosure: { [weak self] error in
                self?.chatContactDataHandler.handleChatContacts(
                    result: .failure(error)
                )
            },
            callbackQueue: queue,
            options: .allNonblocking()
        )

        return provider
    }
}
