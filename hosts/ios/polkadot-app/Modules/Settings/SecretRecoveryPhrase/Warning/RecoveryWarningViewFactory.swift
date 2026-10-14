import Foundation
import SwiftUI
import UIKit_iOS

@MainActor
enum RecoveryWarningViewFactory {
    static func createView(action: @escaping () -> Void) -> RecoveryWarningViewProtocol? {
        let interactor = RecoveryWarningInteractor()
        let wireframe = RecoveryWarningWireframe(action: action)

        let presenter = RecoveryWarningPresenter(
            interactor: interactor,
            wireframe: wireframe
        )

        let view = RecoveryWarningViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        BottomSheetViewFacade.setupBottomSheet(
            from: view.controller,
            preferredHeight: nil
        )

        return view
    }
}
