import UIKit

typealias AuthorizationCompletionBlock = (Bool) -> Void

@MainActor
protocol AuthorizationDismissable: AnyObject {
    func showAuthorizationCompletion(with result: Bool)
}

@MainActor
protocol AuthorizationPresentable: AuthorizationDismissable {
    func authorize(
        animated: Bool,
        retriable: Bool,
        with completionBlock: @escaping AuthorizationCompletionBlock
    )
}

protocol AuthorizationAccessible {
    var isAuthorizing: Bool { get }
}

private let authorization = UUID().uuidString

private enum AuthorizationConstants {
    static var completionBlockKey: String = "io.auth.delegate"
    static var authorizationViewKey: String = "io.auth.view"
}

extension AuthorizationAccessible {
    var isAuthorizing: Bool {
        let view = withUnsafePointer(to: &AuthorizationConstants.authorizationViewKey) {
            objc_getAssociatedObject(
                authorization,
                $0
            ) as? LocalAuthViewProtocol
        }

        return view != nil
    }
}

extension AuthorizationPresentable {
    private var completionBlock: AuthorizationCompletionBlock? {
        get {
            withUnsafePointer(to: &AuthorizationConstants.completionBlockKey) {
                objc_getAssociatedObject(
                    authorization,
                    $0
                ) as? AuthorizationCompletionBlock
            }
        }

        set {
            withUnsafePointer(to: &AuthorizationConstants.completionBlockKey) {
                objc_setAssociatedObject(
                    authorization,
                    $0,
                    newValue,
                    .OBJC_ASSOCIATION_RETAIN
                )
            }
        }
    }

    private var authorizationView: LocalAuthViewProtocol? {
        get {
            withUnsafePointer(to: &AuthorizationConstants.authorizationViewKey) {
                objc_getAssociatedObject(
                    authorization,
                    $0
                ) as? LocalAuthViewProtocol
            }
        }

        set {
            withUnsafePointer(to: &AuthorizationConstants.authorizationViewKey) {
                objc_setAssociatedObject(
                    authorization,
                    $0,
                    newValue,
                    .OBJC_ASSOCIATION_RETAIN
                )
            }
        }
    }

    private var isAuthorizing: Bool {
        authorizationView != nil
    }
}

extension AuthorizationPresentable {
    func authorize(
        animated: Bool,
        with completionBlock: @escaping AuthorizationCompletionBlock
    ) {
        authorize(animated: animated, retriable: false, with: completionBlock)
    }

    func authorize(
        animated: Bool,
        retriable: Bool,
        with completionBlock: @escaping AuthorizationCompletionBlock
    ) {
        // LocalAuth (Face ID / passcode) can't be satisfied headlessly by the
        // triangle-e2e XCUITest driver. Auto-pass for the E2E_TEST artifact,
        // consistent with the existing DISABLE_AUTH / F_DEV dev-build
        // behaviour, so onboarding can create the wallet. No effect on device
        // builds or on a non-E2E simulator build.
        #if DISABLE_AUTH || F_DEV || E2E_TEST
            completionBlock(true)
            return
        #else
            guard !isAuthorizing else {
                return
            }

            guard let presentingController = UIWindow.topWindow?.rootViewController?.topModalViewController else {
                return
            }

            guard let authorizationView = LocalAuthViewFactory.createView(with: self, retriable: retriable) else {
                completionBlock(false)
                return
            }

            self.completionBlock = completionBlock
            self.authorizationView = authorizationView

            authorizationView.controller.modalTransitionStyle = .crossDissolve
            authorizationView.controller.modalPresentationStyle = .overFullScreen
            presentingController.present(authorizationView.controller, animated: animated, completion: nil)
        #endif
    }

    func authorizeInPlace(
        with completionBlock: @escaping AuthorizationCompletionBlock
    ) {
        // LocalAuth (Face ID / passcode) can't be satisfied headlessly by the
        // triangle-e2e XCUITest driver. Auto-pass for the E2E_TEST artifact,
        // consistent with the existing DISABLE_AUTH / F_DEV dev-build
        // behaviour, so onboarding can create the wallet. No effect on device
        // builds or on a non-E2E simulator build.
        #if DISABLE_AUTH || F_DEV || E2E_TEST
            completionBlock(true)
            return
        #else
            guard !isAuthorizing else {
                return
            }

            guard let presentingController = UIWindow.topWindow?.rootViewController?.topModalViewController else {
                return
            }

            guard let authorizationView = LocalAuthViewFactory.createInPlaceView(with: self) else {
                completionBlock(false)
                return
            }
            self.completionBlock = completionBlock
            self.authorizationView = authorizationView

            authorizationView.controller.modalTransitionStyle = .crossDissolve
            authorizationView.controller.modalPresentationStyle = .overCurrentContext
            presentingController.present(authorizationView.controller, animated: true, completion: nil)
        #endif
    }
}

@MainActor
extension AuthorizationPresentable {
    func showAuthorizationCompletion(with result: Bool) {
        guard let completionBlock else {
            return
        }

        self.completionBlock = nil

        guard let authorizationView else {
            return
        }

        authorizationView.controller.presentingViewController?.dismiss(animated: true) {
            self.authorizationView = nil
            completionBlock(result)
        }
    }
}
