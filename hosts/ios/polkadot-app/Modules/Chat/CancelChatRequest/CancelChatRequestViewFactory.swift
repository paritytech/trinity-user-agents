import Foundation

@MainActor
enum CancelChatRequestViewFactory {
    static func createView(
        username: String,
        onConfirm: @escaping () -> Void,
        onKeep: @escaping () -> Void
    ) -> CancelChatRequestViewController {
        let viewController = CancelChatRequestViewController(
            username: username,
            onConfirm: onConfirm,
            onKeep: onKeep
        )

        BottomSheetViewFacade.setupBottomSheet(from: viewController, preferredHeight: 150)

        return viewController
    }
}
