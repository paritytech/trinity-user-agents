import Foundation
import AsyncExtensions
import ChainRegistry
import os

final class ContactsListInteractor {
    weak var presenter: ContactsListInteractorOutputProtocol?

    let chatContactDataProviderFactory: ChatContactDataProviderMaking
    let chatExtensionsRegistry: ChatExtensionsRegistering
    let networkStatusObserver: NetworkStatusObserving
    let scope: ChatListScope
    let userStates: ChatRoomUserStateServicing?
    let searchService: ChatSearchServicing
    let productSearch: ChatListProductSearching
    let logger: LoggerProtocol

    /// The chats that the search of the list goes through: every chat that a list shows, archived rooms included.
    private let searchableChats = OSAllocatedUnfairLock<[ChatWithPeerMetadata]>(initialState: [])
    private var searchTask: Task<Void, Never>?

    private weak var foregroundVisibilityReporter: PushForegroundVisibilityReporting?
    private var task: Task<Void, Never>?

    init(
        chatContactDataProviderFactory: ChatContactDataProviderMaking,
        chatExtensionsRegistry: ChatExtensionsRegistering,
        networkStatusObserver: NetworkStatusObserving,
        foregroundVisibilityReporter: PushForegroundVisibilityReporting?,
        scope: ChatListScope = .main,
        userStates: ChatRoomUserStateServicing? = nil,
        searchService: ChatSearchServicing = ChatSearchService(),
        productSearch: ChatListProductSearching? = nil,
        logger: LoggerProtocol
    ) {
        self.chatContactDataProviderFactory = chatContactDataProviderFactory
        self.chatExtensionsRegistry = chatExtensionsRegistry
        self.networkStatusObserver = networkStatusObserver
        self.foregroundVisibilityReporter = foregroundVisibilityReporter
        self.scope = scope
        self.userStates = userStates
        self.searchService = searchService
        self.productSearch = productSearch ?? ChatListProductSearch(registry: chatExtensionsRegistry, logger: logger)
        self.logger = logger
    }

    deinit {
        task?.cancel()
        searchTask?.cancel()
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

    func entryRoute(for model: ChatOpenModel) async -> ChatExtensionEntryRoute {
        await chatExtensionsRegistry.entryRoute(for: model)
    }

    func search(text: String) {
        searchTask?.cancel()

        let query = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else {
            searchTask = nil
            Task { @MainActor [weak self] in self?.presenter?.didReceive(searchResults: nil) }
            return
        }

        searchTask = Task { [weak self] in
            try? await Task.sleep(for: Self.searchTypingPause)
            guard !Task.isCancelled, let self else { return }

            let results = await searchResults(for: query)
            guard !Task.isCancelled else { return }
            await presenter?.didReceive(searchResults: results)

            // The product-source rooms answer later, so the stored hits show first.
            guard let withProducts = await addingProductHits(to: results), !Task.isCancelled else { return }
            await presenter?.didReceive(searchResults: withProducts)
        }
    }

    func apply(_ change: ChatRoomUserStateChange, to chatId: Chat.Id) {
        guard let userStates else { return }

        Task { [weak self, logger] in
            do {
                try await userStates.apply(change, to: chatId)
            } catch {
                logger.error("The user state of \(chatId.rawRepresentation) did not change: \(error)")
                await self?.presenter?.didReceive(error: error)
            }
        }
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
            let chatsStream = chatContactDataProviderFactory.subscribeChatsWithPredicate(
                .listedChats(productChat: PersonChatMode.isProductChat)
            )
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
        var archivedChatCount = 0
        var searchable: [ChatWithPeerMetadata] = []

        ChatsComparator.chatListOrder(chats).forEach { chat in
            if let listed = listedChat(chat) {
                searchable.append(listed)
            }

            guard scope.shows(chat) else {
                if chatExtensionsRegistry.hasChatExtension(for: chat.chatId.extensionId ?? "") {
                    archivedChatCount += 1
                }
                return
            }

            switch chat.peer {
            case let .person(contact) where contact.hasIncomingChatRequest:
                pendingIncomingRequestCount += 1

                if
                    contact.hasNewIncomingChatRequest {
                    newIncomingRequestCount += 1
                }

            case .person:
                nonIncomingRequestChats.append(chat.chatWithPeerMetadata(using: chatExtensionsRegistry))

            case let .chatExtension(extId, _):
                if chatExtensionsRegistry.hasChatExtension(for: extId) {
                    nonIncomingRequestChats.append(chat.chatWithPeerMetadata(using: chatExtensionsRegistry))
                }
            }
        }

        searchableChats.withLock { $0 = searchable }

        let model = ChatListModel(
            establishedChats: nonIncomingRequestChats,
            pendingIncomingRequestCount: pendingIncomingRequestCount,
            newIncomingRequestCount: newIncomingRequestCount,
            archivedChatCount: scope == .main ? archivedChatCount : 0
        )

        await presenter?.didReceive(model: model)
    }
}

private extension ContactsListInteractor {
    static let searchTypingPause = Duration.milliseconds(200)
    /// How many messages the search of the list shows.
    static let searchHitLimit = 50

    /// The chat as a list shows it, or nil for a chat that no list shows: a chat request or a room whose product
    /// the app does not run.
    func listedChat(_ chat: Chat.LocalModel) -> ChatWithPeerMetadata? {
        switch chat.peer {
        case let .person(contact) where contact.hasIncomingChatRequest:
            return nil
        case .person:
            return chat.chatWithPeerMetadata(using: chatExtensionsRegistry)
        case let .chatExtension(extensionId, _):
            guard chatExtensionsRegistry.hasChatExtension(for: extensionId) else {
                return nil
            }
            return chat.chatWithPeerMetadata(using: chatExtensionsRegistry)
        }
    }

    func searchResults(for query: String) async -> ChatListSearchResults {
        let chats = searchableChats.withLock { $0 }
        let normalizedQuery = ChatSearchText.normalized(query)
        let named = chats.filter { ChatSearchText.normalized($0.peerMetadata.name).contains(normalizedQuery) }

        // A product-source room keeps its messages in the product: `addingProductHits` asks its product.
        let stored = chats.filter { $0.chat.roomPolicy?.messageStore != .product }
        let byChatId = Dictionary(stored.map { ($0.chat.chatId, $0) }, uniquingKeysWith: { first, _ in first })

        let hits: [ChatSearchHit]
        do {
            hits = try await searchService.searchMessages(
                matching: query,
                in: Set(byChatId.keys),
                limit: Self.searchHitLimit
            )
        } catch {
            logger.error("The search of the chat list failed: \(error)")
            hits = []
        }

        return ChatListSearchResults(
            query: query,
            chats: named,
            hits: hits.compactMap { hit in byChatId[hit.chatId].map { ChatListMessageHit(hit: hit, chat: $0) } }
        )
    }

    /// The results with the hits of the product-source rooms that declare `search`, newest rooms first. Nil when no
    /// such room is in the list.
    func addingProductHits(to results: ChatListSearchResults) async -> ChatListSearchResults? {
        let rooms = searchableChats.withLock { $0 }.filter { chat in
            guard let policy = chat.chat.roomPolicy else { return false }
            return policy.messageStore == .product && policy.offersSearch
        }
        guard !rooms.isEmpty else { return nil }

        let byChatId = Dictionary(rooms.map { ($0.chat.chatId, $0) }, uniquingKeysWith: { first, _ in first })
        let hits = await productSearch.search(results.query, in: rooms.map(\.chat.chatId))
        return results.adding(productHits: hits, chats: byChatId, limit: Self.searchHitLimit)
    }
}
