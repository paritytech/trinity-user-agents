import UIKit
import SafariServices

public struct WebPresentableStyle {
    public enum Mode {
        case automatic
        case modal(ModalStyle)

        public enum ModalStyle {
            case fullScreen
            case compact

            fileprivate var presentationStyle: UIModalPresentationStyle {
                switch self {
                case .fullScreen: .overFullScreen
                case .compact: .automatic
                }
            }
        }
    }

    let mode: Mode
    let controlTintColor: UIColor?
    let barTintColor: UIColor?

    public init(
        mode: Mode,
        controlTintColor: UIColor? = nil,
        barTintColor: UIColor? = nil
    ) {
        self.mode = mode
        self.controlTintColor = controlTintColor
        self.barTintColor = barTintColor
    }
}

public protocol WebPresentable: AnyObject {
    func prewarmURLs(_ urls: [URL?]) -> SFSafariViewController.PrewarmingToken
    @MainActor
    func showWeb(url: URL, from view: ControllerBackedProtocol, style: WebPresentableStyle)
}

public extension WebPresentable {
    var supportedSafariScheme: [String] {
        ["https", "http"]
    }

    func prewarmURLs(_ urls: [URL?]) -> SFSafariViewController.PrewarmingToken {
        SFSafariViewController.prewarmConnections(to: urls.compactMap { $0 })
    }

    @MainActor
    func showWeb(
        url: URL,
        from view: ControllerBackedProtocol,
        style: WebPresentableStyle
    ) {
        showWeb(url: url, from: view.controller, style: style)
    }

    func showWeb(
        url: URL,
        from viewController: UIViewController,
        style: WebPresentableStyle
    ) {
        guard let scheme = url.scheme, supportedSafariScheme.contains(scheme) else {
            return
        }

        let webController = WebViewFactory.createWebViewController(for: url, style: style)
        viewController.present(webController, animated: true, completion: nil)
    }
}

public enum WebViewFactory {
    static func createWebViewController(for url: URL, style: WebPresentableStyle) -> UIViewController {
        let webController = SFSafariViewController(url: url)

        if let controlTintColor = style.controlTintColor {
            webController.preferredControlTintColor = controlTintColor
        }

        if let barTintColor = style.barTintColor {
            webController.preferredBarTintColor = barTintColor
        }

        switch style.mode {
        case let .modal(style):
            webController.modalPresentationStyle = style.presentationStyle
        default:
            break
        }

        return webController
    }
}
