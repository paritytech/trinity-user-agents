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
    /// The rows of the participants that came, one for each participant. A page adds the rows of its own
    /// participants only, so the work of a page does not grow with the length of the list (P7.1, B1).
    private var rows: [ChatParticipantRowViewModel] = []
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
        rows += page.map(row(for:))
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
    func row(for participant: ChatRoomParticipant) -> ChatParticipantRowViewModel {
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

    func provideViewModel() {
        #if targetEnvironment(simulator)
            if TrUAPIE2EMarkers.isEnabled {
                Logger.shared
                    .debug("[e2e:ui] participants-list rows=\(rows.count) loading=\(isLoading) failed=\(failed)")
            }
        #endif
        view?.didReceive(ChatParticipantsViewModel(rows: rows, isLoading: isLoading, failed: failed))
    }
}
