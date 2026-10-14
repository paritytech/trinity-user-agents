import Foundation
import Foundation_iOS

@MainActor
final class LocalAuthWireframe: LocalAuthWireframeProtocol, BottomSheetMessagePresentable {
    let authDismissable: AuthorizationDismissable

    init(authDismissable: AuthorizationDismissable) {
        self.authDismissable = authDismissable
    }

    lazy var rootAnimator = RootControllerAnimationCoordinator()

    func complete(with isSuccess: Bool) {
        authDismissable.showAuthorizationCompletion(with: isSuccess)
    }

    func showAuthFailed(from view: LocalAuthViewProtocol?, completion: @escaping () -> Void) {
        guard let view else {
            return
        }

        showBottomSheet(
            from: view,
            viewModel: .init(
                graphics: nil,
                title: LocalizableResource { _ in String(localized: .authFailedTitle).uppercased() },
                message: LocalizableResource { _ in .normal(String(localized: .authFailedMessage)) },
                mainAction: .init(
                    title: LocalizableResource { _ in String(localized: .Common.gotIt) },
                    handler: completion
                ),
                secondaryAction: nil
            ),
            allowsSwipesDown: false,
            preferredHeight: 289
        )
    }
}
