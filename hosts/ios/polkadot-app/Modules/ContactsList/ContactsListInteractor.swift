import Foundation
import AsyncExtensions
import ChainRegistry

final class ContactsListInteractor {
    weak var presenter: ContactsListInteractorOutputProtocol?

    let chatContactDataProviderFactory: ChatContactDataProviderMaking
    let chatExtensionsRegistry: ChatExtensionsRegistering
    let networkStatusObserver: NetworkStatusObserving
    let logger: LoggerProtocol

    private weak var foregroundVisibilityReporter: PushForegroundVisibilityReporting?
    private var task: Task<Void, Never>?

    init(
        chatContactDataProviderFactory: ChatContactDataProviderMaking,
        chatExtensionsRegistry: ChatExtensionsRegistering,
        networkStatusObserver: NetworkStatusObserving,
        foregroundVisibilityReporter: PushForegroundVisibilityReporting?,
        logger: LoggerProtocol
    ) {
        self.chatContactDataProviderFactory = chatContactDataProviderFactory
        self.chatExtensionsRegistry = chatExtensionsRegistry
        self.networkStatusObserver = networkStatusObserver
        self.foregroundVisibilityReporter = foregroundVisibilityReporter
        self.logger = logger
    }

    deinit {
        task?.cancel()
    }
}

extension ContactsListInteractor: ContactsListInteractorInputProtocol {
    func setup() {
        subscribeForChanges()
        subscribeNetworkStatus()
    }

    func notifyViewAppeared() {
        foregroundVisibilityReporter?.updateVisibleScreen(.chatList)
    }

    func notifyViewDisappeared() {
        foregroundVisibilityReporter?.updateVisibleScreen(.other)
    }

    func entryRoute(for model: ChatOpenModel) async throws -> ChatExtensionEntryRoute {
        try await chatExtensionsRegistry.entryRoute(for: model)
    }
}

private enum ListChangeEvent {
    case chatsUpdated([Chat.LocalModel])
    case extensionsChanged
}

private extension ContactsListInteractor {
    func subscribeNetworkStatus() {
        networkStatusObserver.start { [weak self] status in
            self?.presenter?.didReceive(networkStatus: status)
        }
    }

    func subscribeForChanges() {
        task = Task { [weak self, chatContactDataProviderFactory, chatExtensionsRegistry, logger] in
            let chatsStream = chatContactDataProviderFactory.subscribeChatsWithPredicate(.listedChats())
            let extensionChanges = chatExtensionsRegistry.onChangeStream

            var latestChats: [Chat.LocalModel] = []

            // peer metadata now depends on bot changes thus we need to update the list
            let chatsEvents = chatsStream.map { ListChangeEvent.chatsUpdated($0) }.eraseToAnyAsyncSequence()
            let extensionEvents = extensionChanges.map { _ in ListChangeEvent.extensionsChanged }
                .eraseToAnyAsyncSequence()

            do {
                for try await event in merge(chatsEvents, extensionEvents) {
                    if case let .chatsUpdated(chats) = event {
                        latestChats = chats
                    }

                    await self?.handleChats(latestChats)
                }

                logger.debug("Streams ended")
            } catch {
                logger.error("Unexpected error: \(error)")
                await self?.presenter?.didReceive(error: error)
            }
        }
    }

    func handleChats(_ chats: [Chat.LocalModel]) async {
        var newIncomingRequestCount = 0
        var pendingIncomingRequestCount = 0

        var nonIncomingRequestChats: [ChatWithPeerMetadata] = []

        chats.forEach { chat in
            switch chat.peer {
            case .person:
                // The list holds no person chat of the native chat: the chat product runs them (HX2).
                break

            case let .chatExtension(extId, _):
                guard chatExtensionsRegistry.hasChatExtension(for: extId) else {
                    break
                }

                // A room of the chat product is a person chat: its request waits behind "New requests", and the
                // list shows no invite and no blocked contact.
                switch chat.roomContact {
                case let contact? where contact.state == .incoming:
                    pendingIncomingRequestCount += 1

                    // A request is new while its message is new, as `Chat.Contact.hasNewIncomingChatRequest` reads.
                    if chat.message?.status == .incoming(.new) {
                        newIncomingRequestCount += 1
                    }
                case let contact? where !contact.isListed:
                    break
                case nil where BundledChatProductResolver.isChatProduct(extId):
                    // The chat product lists its contacts only, as the native chat lists its person chats.
                    break
                default:
                    nonIncomingRequestChats.append(chat.chatWithPeerMetadata(using: chatExtensionsRegistry))
                }
            }
        }

        let model = ChatListModel(
            establishedChats: nonIncomingRequestChats,
            pendingIncomingRequestCount: pendingIncomingRequestCount,
            newIncomingRequestCount: newIncomingRequestCount
        )

        await presenter?.didReceive(model: model)
    }
}
