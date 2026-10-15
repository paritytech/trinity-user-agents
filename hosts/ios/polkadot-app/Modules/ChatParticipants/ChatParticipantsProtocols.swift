import Foundation
import PolkadotUI
import UIKitExt

@MainActor
protocol ChatParticipantsViewProtocol: ControllerBackedProtocol {
    func didReceive(_ viewModel: ChatParticipantsViewModel)
}

@MainActor
protocol ChatParticipantsPresenterProtocol: AnyObject {
    func setup()
    /// The list reached its end on the screen, so the next page can come.
    func loadMore()
    func selectParticipant(at index: Int)
}

@MainActor
protocol ChatParticipantsInteractorInputProtocol: AnyObject {
    /// Asks for the page after the pages that came. Does nothing while a page comes or after the last page.
    func loadNextPage()
}

@MainActor
protocol ChatParticipantsInteractorOutputProtocol: AnyObject {
    func didReceive(page: [ChatRoomParticipant], isLast: Bool)
    func didFailToLoadPage()
}

@MainActor
protocol ChatParticipantsWireframeProtocol: AnyObject {
    func showParticipant(_ participant: ChatRoomParticipant, from view: ChatParticipantsViewProtocol?)
}

/// What the participant list shows.
struct ChatParticipantsViewModel: Equatable {
    let rows: [ChatParticipantRowViewModel]
    /// A page comes now.
    let isLoading: Bool
    /// The last page did not come. A scroll to the end asks again.
    let failed: Bool
}
