import Foundation
import Foundation_iOS
import UIKitExt
import PolkadotUI

@MainActor
protocol BottomSheetMessagePresentable {
    func showBottomSheet(
        from view: ControllerBackedProtocol,
        viewModel: TitleDetailsSheetViewModel,
        allowsSwipesDown: Bool,
        preferredHeight: CGFloat
    )
}

@MainActor
extension BottomSheetMessagePresentable {
    func showMessageBottomSheet(
        from view: ControllerBackedProtocol,
        title: String,
        message: String,
        preferredHeight: CGFloat
    ) {
        showMessageBottomSheet(
            from: view,
            title: title,
            message: message,
            close: String(localized: .Common.gotIt).uppercased(),
            preferredHeight: preferredHeight
        )
    }

    func showMessageBottomSheet(
        from view: ControllerBackedProtocol,
        title: String,
        message: String,
        close: String,
        preferredHeight: CGFloat
    ) {
        let viewModel = TitleDetailsSheetViewModel(
            graphics: nil,
            title: LocalizableResource { _ in title },
            message: LocalizableResource { _ in .normal(message) },
            mainAction: .init(
                title: LocalizableResource { _ in close },
                handler: {}
            ),
            secondaryAction: nil
        )

        showBottomSheet(
            from: view,
            viewModel: viewModel,
            allowsSwipesDown: true,
            preferredHeight: preferredHeight
        )
    }

    func showBottomSheet(
        from view: ControllerBackedProtocol,
        viewModel: TitleDetailsSheetViewModel,
        allowsSwipesDown: Bool,
        preferredHeight: CGFloat
    ) {
        let infoView = TitleDetailsSheetViewFactory.createView(
            from: viewModel,
            styler: DeviceStatusSheetStyler(),
            allowsSwipeDown: allowsSwipesDown
        )

        BottomSheetViewFacade.setupBottomSheet(from: infoView.controller, preferredHeight: preferredHeight)

        view.controller.present(infoView.controller, animated: true)
    }
}
