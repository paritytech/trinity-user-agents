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
    let logger: LoggerProtocol

    private var chatsByIdentifier: [String: ChatWithPeerMetadata] = [:]
    private var openChatTask: Task<Void, Never>?

    init(
        interactor: ContactsListInteractorInputProtocol,
        wireframe: ContactsListWireframeProtocol,
        viewModelFactory: ContactsListViewModelMaking,
        titleViewModelFactory: NetworkStatusTitleViewModelMaking,
        assetDisplayInfo: AssetBalanceDisplayInfo,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        self.titleViewModelFactory = titleViewModelFactory
        self.assetDisplayInfo = assetDisplayInfo
        self.logger = logger
    }
}

extension ContactsListPresenter: ContactsListPresenterProtocol {
    func setup() {
        didReceive(networkStatus: .connected)
        interactor.setup()
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
}

extension ContactsListPresenter: ContactsListInteractorOutputProtocol {
    func didReceive(model: ChatListModel) {
        model.establishedChats.forEach {
            chatsByIdentifier[$0.chat.identifier] = $0
        }

        provideViewModel(model)
    }

    func didReceive(networkStatus: NetworkStatus) {
        let titleViewModel = titleViewModelFactory.createTitleViewModel(for: networkStatus)
        view?.didReceive(titleViewModel: titleViewModel)
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
        let viewModel = viewModelFactory.createViewModel(
            assetDisplayInfo: assetDisplayInfo,
            model: model
        )
        view?.didReceive(viewModel: viewModel)
    }
}
