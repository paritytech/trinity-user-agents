import Foundation
import UIKit

@MainActor
enum ChatParticipantsViewFactory {
    static func createView(
        productRoom: ChatProductRoomPresenter,
        iconViewModelFactory: ProductIconViewModelMaking
    ) -> ChatParticipantsViewProtocol {
        let interactor = ChatParticipantsInteractor(directory: productRoom.participants)
        let wireframe = ChatParticipantsWireframe(productRoom: productRoom)
        let presenter = ChatParticipantsPresenter(
            interactor: interactor,
            wireframe: wireframe,
            appearanceFactory: productRoom.appearanceFactory,
            iconViewModelFactory: iconViewModelFactory
        )
        let view = ChatParticipantsViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
