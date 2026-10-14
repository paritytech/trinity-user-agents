import UIKit

public struct AlertPresentableAction {
    public enum Style {
        case normal
        case destructive
        case cancel
    }

    let title: String
    let handler: (() -> Void)?
    let style: Style

    public init(title: String, style: Style = .normal, handler: @escaping () -> Void) {
        self.title = title
        self.handler = handler
        self.style = style
    }

    public init(title: String, style: Style = .normal) {
        self.title = title
        self.style = style
        handler = nil
    }
}

public struct AlertPresentableViewModel {
    let title: String?
    let message: String?
    let actions: [AlertPresentableAction]
    let closeAction: String?

    public init(
        title: String? = nil,
        message: String? = nil,
        actions: [AlertPresentableAction],
        closeActionTitle: String? = nil
    ) {
        self.title = title
        self.message = message
        self.actions = actions
        closeAction = closeActionTitle
    }
}

@MainActor
public protocol AlertPresentable: AnyObject {
    func present(
        message: String?,
        title: String?,
        closeAction: String?,
        from view: ControllerBackedProtocol?
    )

    func present(
        viewModel: AlertPresentableViewModel,
        style: UIAlertController.Style,
        from view: ControllerBackedProtocol?
    )
}

public extension AlertPresentableAction.Style {
    var uialertStyle: UIAlertAction.Style {
        switch self {
        case .normal:
            .default
        case .cancel:
            .cancel
        case .destructive:
            .destructive
        }
    }
}

public extension AlertPresentable {
    func present(
        message: String?,
        title: String?,
        closeAction: String?,
        from view: ControllerBackedProtocol?
    ) {
        var currentController = view?.controller

        if currentController == nil {
            currentController = UIWindow.topWindow?.rootViewController
        }

        guard let controller = currentController else {
            return
        }

        UIAlertController.present(
            message: message,
            title: title,
            closeAction: closeAction,
            with: controller
        )
    }

    func present(
        viewModel: AlertPresentableViewModel,
        style: UIAlertController.Style,
        from view: ControllerBackedProtocol?
    ) {
        var currentController = view?.controller

        if currentController == nil {
            currentController = UIWindow.topWindow?.rootViewController
        }

        guard let controller = currentController else {
            return
        }

        let alertView = UIAlertController(
            title: viewModel.title,
            message: viewModel.message,
            preferredStyle: style
        )

        for action in viewModel.actions {
            let alertAction = UIAlertAction(title: action.title, style: action.style.uialertStyle) { _ in
                action.handler?()
            }

            alertView.addAction(alertAction)
        }

        if let closeAction = viewModel.closeAction {
            let action = UIAlertAction(
                title: closeAction,
                style: .cancel,
                handler: nil
            )
            alertView.addAction(action)
        }

        controller.present(alertView, animated: true)
    }
}

public extension UIAlertController {
    static func present(
        message: String?,
        title: String?,
        closeAction: String?,
        with presenter: UIViewController
    ) {
        let alertView = UIAlertController(title: title, message: message, preferredStyle: .alert)
        let action = UIAlertAction(title: closeAction, style: .cancel, handler: nil)
        alertView.addAction(action)
        presenter.present(alertView, animated: true, completion: nil)
    }
}
