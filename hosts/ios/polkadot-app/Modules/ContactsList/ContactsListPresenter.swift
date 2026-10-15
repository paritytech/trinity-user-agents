import Foundation
import PolkadotUI
import ChainRegistry

final class ContactsListPresenter {
    weak var view: ContactsListViewProtocol?
    let wireframe: ContactsListWireframeProtocol
    let interactor: ContactsListInteractorInputProtocol
    let viewModelFactory: ContactsListViewModelMaking
    let titleViewModelFactory: NetworkStatusTitleViewModelMaking
    let assetDisplayInfo: AssetBalanceDisplayInfo
    let scope: ChatListScope
    let logger: LoggerProtocol

    private var chatsByIdentifier: [String: ChatWithPeerMetadata] = [:]
    private var openChatTask: Task<Void, Never>?
    private var muteEndTask: Task<Void, Never>?
    /// What the search field found. Nil while the field is empty: the list shows its chats.
    var searchResults: ChatListSearchResults?
    private var searchHitsByIdentifier: [String: ChatListMessageHit] = [:]
    /// The last model, which the E2E harness draws again (S6).
    var lastModel: ChatListModel?
    var e2eObserver: NSObjectProtocol?
    /// Draws the list again when a product room rings (P10.7).
    var callsTask: Task<Void, Never>?

    init(
        interactor: ContactsListInteractorInputProtocol,
        wireframe: ContactsListWireframeProtocol,
        viewModelFactory: ContactsListViewModelMaking,
        titleViewModelFactory: NetworkStatusTitleViewModelMaking,
        assetDisplayInfo: AssetBalanceDisplayInfo,
        scope: ChatListScope = .main,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        self.titleViewModelFactory = titleViewModelFactory
        self.assetDisplayInfo = assetDisplayInfo
        self.scope = scope
        self.logger = logger
    }

    deinit {
        callsTask?.cancel()
    }
}

extension ContactsListPresenter: ContactsListPresenterProtocol {
    func setup() {
        didReceive(networkStatus: .connected)
        interactor.setup()
        callsTask = followRoomCalls()
        #if targetEnvironment(simulator)
            e2eObserver = observeE2EListRender()
        #endif
    }

    func viewWillAppear() {
        interactor.notifyViewAppeared()
    }

    func viewWillDisappear() {
        interactor.notifyViewDisappeared()
    }

    func showScanPanel() {
        wireframe.showScanPanel()
    }

    func openChat(contactIdentifier: String) {
        if let hit = searchHitsByIdentifier[contactIdentifier] {
            wireframe.showChat(from: view, for: .existingChat(hit.hit.chatId), showing: hit.hit.messageId)
            return
        }

        guard let chat = chatsByIdentifier[contactIdentifier]?.chat else {
            return assertionFailure()
        }

        let openModel = ChatOpenModel.existingChat(chat.chatId)

        openChatTask?.cancel()
        openChatTask = Task { @MainActor [weak self] in
            guard let self else {
                return
            }

            let route = await interactor.entryRoute(for: openModel)

            guard !Task.isCancelled else {
                return
            }

            open(route)
        }
    }

    func showIncomingRequests() {
        wireframe.showIncomingRequests(from: view)
    }

    func showArchivedChats() {
        wireframe.showArchivedChats(from: view)
    }

    func rowActions(for contactIdentifier: String) -> ChatListRowActions? {
        guard
            let chat = chatsByIdentifier[contactIdentifier]?.chat,
            chat.chatId.roomId != nil,
            let state = chat.roomDeclarations?.userState
        else {
            return nil
        }

        return ChatListRowActions(isMuted: state.isMuted(at: Date()), isArchived: state.archived)
    }

    func apply(_ change: ChatRoomUserStateChange, contactIdentifier: String) {
        guard let chat = chatsByIdentifier[contactIdentifier]?.chat else { return }

        interactor.apply(change, to: chat.chatId)
    }

    func search(text: String) {
        interactor.search(text: text)
    }

    func requestMute(contactIdentifier: String) {
        wireframe.showMuteOptions(from: view) { [weak self] duration in
            self?.apply(.mute(duration), contactIdentifier: contactIdentifier)
        }
    }
}

extension ContactsListPresenter: ContactsListInteractorOutputProtocol {
    func didReceive(model: ChatListModel) {
        chatsByIdentifier = Dictionary(
            model.establishedChats.map { ($0.chat.identifier, $0) },
            uniquingKeysWith: { _, newest in newest }
        )
        for chat in searchResults?.chats ?? [] {
            chatsByIdentifier[chat.chat.identifier] = chat
        }

        provideViewModel(model)
    }

    func didReceive(networkStatus: NetworkStatus) {
        let titleViewModel = titleViewModelFactory.createTitleViewModel(for: networkStatus)
        view?.didReceive(titleViewModel: titleViewModel)
    }

    func didReceive(searchResults: ChatListSearchResults?) {
        self.searchResults = searchResults
        searchHitsByIdentifier = Dictionary(
            (searchResults?.hits ?? []).map { ($0.identifier, $0) },
            uniquingKeysWith: { first, _ in first }
        )
        for chat in searchResults?.chats ?? [] {
            chatsByIdentifier[chat.chat.identifier] = chat
        }

        guard let lastModel else { return }
        provideViewModel(lastModel)
        #if targetEnvironment(simulator)
            markSearchResults()
        #endif
    }

    func didReceive(error: Error) {
        logger.error("Unexpected error: \(error)")

        _ = wireframe.present(error: error, from: view)
    }
}

extension ContactsListPresenter {
    @MainActor
    func open(_ route: ChatExtensionEntryRoute) {
        switch route {
        case let .chat(model):
            wireframe.showChat(from: view, for: model)
        case let .deepLink(url):
            wireframe.open(url: url)
        }
    }

    func provideViewModel(_ model: ChatListModel) {
        lastModel = model
        let viewModel =
            if let searchResults {
                viewModelFactory.createSearchViewModel(assetDisplayInfo: assetDisplayInfo, results: searchResults)
            } else {
                viewModelFactory.createViewModel(assetDisplayInfo: assetDisplayInfo, model: model)
            }
        view?.didReceive(viewModel: viewModel)
        scheduleRedrawAtMuteEnd(model)
        #if targetEnvironment(simulator)
            markProductRooms(model)
        #endif
    }

    /// A mute with an end goes away without a change in the store, so the list draws again when the first mute
    /// ends.
    func scheduleRedrawAtMuteEnd(_ model: ChatListModel) {
        muteEndTask?.cancel()

        let now = Date().toChatTimestamp()
        let firstEnd = model.establishedChats
            .compactMap { $0.chat.roomDeclarations?.userState.mutedUntil }
            .filter { $0 != Chat.RoomUserState.mutedForever && $0 > now }
            .min()
        guard let firstEnd else { return }

        muteEndTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(firstEnd - now))
            guard !Task.isCancelled, let self, let lastModel else { return }
            provideViewModel(lastModel)
        }
    }
}
