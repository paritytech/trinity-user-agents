import Foundation
import PolkadotUI
import Foundation_iOS

final class SharePresenter {
    weak var view: ShareViewProtocol?

    let interactor: ShareInteractorInputProtocol
    let wireframe: ShareWireframeProtocol
    let viewModelFactory: ShareViewModelFactoryProtocol
    let items: [ShareItem]
    let logger: LoggerProtocol

    private var chats: [ChatWithPeerMetadata] = []
    private var selectedChatIds: Set<Chat.Id> = []
    private var isLoading: Bool = false

    init(
        items: [ShareItem],
        interactor: ShareInteractorInputProtocol,
        wireframe: ShareWireframeProtocol,
        viewModelFactory: ShareViewModelFactoryProtocol,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.items = items
        self.interactor = interactor
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        self.logger = logger
    }
}

extension SharePresenter: SharePresenterProtocol {
    func setup() {
        interactor.setup()
        updateView()
    }

    func didTapShare() {
        guard !selectedChatIds.isEmpty, !isLoading else { return }
        interactor.send(items: items, userMessage: nil, to: Array(selectedChatIds))
    }

    func didTapCancel() {
        guard !isLoading else { return }
        wireframe.close(from: view)
    }

    func didTapSystemShare() {
        guard !isLoading else { return }
        wireframe.presentSystemShare(items: items, from: view)
    }

    func didToggleSelection(chatId: Chat.Id, isSelected: Bool) {
        guard !isLoading else { return }

        if isSelected {
            selectedChatIds.insert(chatId)
        } else {
            selectedChatIds.remove(chatId)
        }
        updateView()
    }
}

extension SharePresenter: ShareInteractorOutputProtocol {
    func didReceive(chats: [ChatWithPeerMetadata]) {
        self.chats = chats

        let validIds = Set(chats.map(\.chat.chatId))
        selectedChatIds.formIntersection(validIds)

        updateView()
    }

    func didReceive(error: Error) {
        logger.error("Share error: \(error)")
        _ = wireframe.present(error: error, from: view)
    }

    func didReceive(isLoading: Bool) {
        self.isLoading = isLoading
        updateView()
    }

    func didCompleteSend() {
        wireframe.close(from: view)
    }
}

private extension SharePresenter {
    func updateView() {
        let onSelection: (Chat.Id, Bool) -> Void = { [weak self] chatId, newValue in
            MainActor.assumeIsolated {
                self?.didToggleSelection(chatId: chatId, isSelected: newValue)
            }
        }

        let viewModel = viewModelFactory.createViewModel(
            chats: chats,
            selectedIds: selectedChatIds,
            isLoading: isLoading,
            onSelection: onSelection
        )

        view?.didReceive(viewModel: viewModel)
    }
}
