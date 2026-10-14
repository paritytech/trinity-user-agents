import Foundation
import PolkadotUI

@MainActor
final class ChatParticipantsPresenter {
    weak var view: ChatParticipantsViewProtocol?
    let interactor: ChatParticipantsInteractorInputProtocol
    let wireframe: ChatParticipantsWireframeProtocol

    private let appearanceFactory: ChatParticipantAppearanceFactory
    private let iconViewModelFactory: ProductIconViewModelMaking
    private var participants: [ChatRoomParticipant] = []
    private var isLoading = false
    private var failed = false

    init(
        interactor: ChatParticipantsInteractorInputProtocol,
        wireframe: ChatParticipantsWireframeProtocol,
        appearanceFactory: ChatParticipantAppearanceFactory,
        iconViewModelFactory: ProductIconViewModelMaking
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.appearanceFactory = appearanceFactory
        self.iconViewModelFactory = iconViewModelFactory
    }
}

extension ChatParticipantsPresenter: ChatParticipantsPresenterProtocol {
    func setup() {
        loadMore()
    }

    func loadMore() {
        isLoading = true
        failed = false
        interactor.loadNextPage()
        provideViewModel()
    }

    func selectParticipant(at index: Int) {
        guard participants.indices.contains(index) else { return }
        wireframe.showParticipant(participants[index], from: view)
    }
}

extension ChatParticipantsPresenter: ChatParticipantsInteractorOutputProtocol {
    func didReceive(page: [ChatRoomParticipant], isLast _: Bool) {
        participants += page
        isLoading = false
        provideViewModel()
    }

    func didFailToLoadPage() {
        isLoading = false
        failed = true
        provideViewModel()
    }
}

private extension ChatParticipantsPresenter {
    func provideViewModel() {
        let rows = participants.map { participant in
            let appearance = appearanceFactory.appearance(of: participant)
            let icon: (any ImageViewModelProtocol)? =
                if let image = appearance.image {
                    iconViewModelFactory.createViewModel(forChatResource: image)
                } else if let domain = appearance.productDomain {
                    iconViewModelFactory.createViewModel(for: domain)
                } else {
                    nil
                }

            return ChatParticipantRowViewModel(
                participantId: participant.participantId,
                name: appearance.name,
                detail: appearance.detail,
                avatar: .colored(text: appearance.initials, colorSeed: participant.participantId),
                icon: icon
            )
        }

        view?.didReceive(ChatParticipantsViewModel(rows: rows, isLoading: isLoading, failed: failed))
    }
}
