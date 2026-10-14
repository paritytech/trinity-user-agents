import Foundation

@MainActor
enum BlockUserViewFactory {
    static func createView(
        username: String,
        onBlock: @escaping () -> Void,
        onCancel: @escaping () -> Void
    ) -> BlockUserViewController {
        let viewController = BlockUserViewController(
            username: username,
            onBlock: onBlock,
            onCancel: onCancel
        )

        BottomSheetViewFacade.setupBottomSheet(from: viewController, preferredHeight: nil)

        return viewController
    }
}
