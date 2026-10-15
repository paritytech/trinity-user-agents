import Foundation
import Testing
import UIKitExt
import UIKit
@testable import polkadot_app

/// The participant list adds the rows of each page only, so the work of a page does not grow with the list
/// (P7.1, B1).
@MainActor
struct ChatParticipantsListTests {
    private let icons = RecordingProductIconViewModelFactory()

    @Test func eachPageAddsItsOwnRowsAfterTheRowsThatCame() {
        let (presenter, view) = makePresenter()

        presenter.didReceive(page: participants(0 ..< 100), isLast: false)
        presenter.loadMore()
        presenter.didReceive(page: participants(100 ..< 200), isLast: true)

        let rows = view.viewModels.last?.rows ?? []
        #expect(rows.map(\.participantId) == (0 ..< 200).map { "p\($0)" })
        #expect(view.viewModels.map(\.rows.count) == [100, 100, 200])
        #expect(view.viewModels.map(\.isLoading) == [false, true, false])
        // One icon for each participant: the second page built no row of the first page again.
        #expect(icons.requestedDomains.count == 200)
    }
}

@MainActor
private final class RecordingParticipantsView: ChatParticipantsViewProtocol {
    private(set) var viewModels: [ChatParticipantsViewModel] = []
    let controller = UIViewController()
    let isSetup = true

    func didReceive(_ viewModel: ChatParticipantsViewModel) {
        viewModels.append(viewModel)
    }
}

@MainActor
private final class IdleParticipantsInteractor: ChatParticipantsInteractorInputProtocol {
    func loadNextPage() {}
}

@MainActor
private final class IdleParticipantsWireframe: ChatParticipantsWireframeProtocol {
    func showParticipant(_: ChatRoomParticipant, from _: ChatParticipantsViewProtocol?) {}
}

private extension ChatParticipantsListTests {
    func makePresenter() -> (ChatParticipantsPresenter, RecordingParticipantsView) {
        let view = RecordingParticipantsView()
        let presenter = ChatParticipantsPresenter(
            interactor: IdleParticipantsInteractor(),
            wireframe: IdleParticipantsWireframe(),
            appearanceFactory: ChatParticipantAppearanceFactory(
                myUsername: "alice",
                productName: "Sample",
                productId: "s.dot"
            ),
            iconViewModelFactory: icons
        )
        presenter.view = view
        return (presenter, view)
    }

    func participants(_ range: Range<Int>) -> [ChatRoomParticipant] {
        range.map { ChatRoomParticipant(participantId: "p\($0)", kind: .product(productId: "guest\($0).dot")) }
    }
}
