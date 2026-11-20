import Foundation

@MainActor
enum ChatRequestConfirmationViewFactory {
    static func createView(
        viewModel: ChatRequestConfirmationViewModel,
        onConfirm: @escaping () -> Void
    ) -> ChatRequestConfirmationViewController {
        let viewController = ChatRequestConfirmationViewController(
            viewModel: viewModel,
            onConfirm: onConfirm
        )

        BottomSheetViewFacade.setupBottomSheet(from: viewController, preferredHeight: 150)

        return viewController
    }
}
