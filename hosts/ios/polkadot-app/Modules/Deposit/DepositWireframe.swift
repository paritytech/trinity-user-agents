import Foundation
import Foundation_iOS
import UIKitExt
import PolkadotUI

final class DepositWireframe: DepositWireframeProtocol {
    let completion: () -> Void
    init(completion: @escaping () -> Void) {
        self.completion = completion
    }

    func close(view: DepositViewProtocol?) {
        view?.controller.dismiss(animated: true)
    }

    func doneFunding(view: DepositViewProtocol?) {
        view?.controller.dismiss(animated: true, completion: completion)
    }

    func showDismissConfirmation(
        view: (any ControllerBackedProtocol)?,
        viewModel: TitleDetailsSheetViewModel
    ) {
        let buttonStyler = MessageSheetStyler.RoundedButtonFactory(
            mainStyle: .white,
            secondaryStyle: .mainDark
        )

        let infoView = TitleDetailsSheetViewFactory.createView(
            from: viewModel,
            styler: MessageSheetStyler(controlFactory: buttonStyler),
            allowsSwipeDown: false
        )

        BottomSheetViewFacade.setupBottomSheet(from: infoView.controller, preferredHeight: nil)

        view?.controller.present(infoView.controller, animated: true)
    }
}
