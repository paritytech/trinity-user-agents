import Foundation

@MainActor
enum LeaveChatViewFactory {
    static func createView(
        username: String,
        onDelete: @escaping () -> Void,
        onCancel: @escaping () -> Void
    ) -> LeaveChatViewController {
        let viewController = LeaveChatViewController(
            username: username,
            onDelete: onDelete,
            onCancel: onCancel
        )

        BottomSheetViewFacade.setupBottomSheet(from: viewController, preferredHeight: 150)

        return viewController
    }
}
