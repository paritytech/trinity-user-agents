import Foundation
import UIKit
import UIKitExt

/// The navigation of the parts of a product room that the product declares (P6).
@MainActor
protocol ChatProductRoomWireframing: AnyObject {
    /// Asks the user to confirm a link of a product action, then opens it (02-DESIGN §6.6, scenario S27).
    func confirmProductLink(_ url: URL, from view: ControllerBackedProtocol?, onOpen: @escaping () -> Void)

    /// Opens a link of a product action: a `polkadot:` link through the link pipeline of the app, an `https` link
    /// in the in-app browser, and any other link through the system.
    func openProductLink(_ url: URL, from view: ControllerBackedProtocol?)
}

extension ChatWireframe: ChatProductRoomWireframing {
    func confirmProductLink(_ url: URL, from view: ControllerBackedProtocol?, onOpen: @escaping () -> Void) {
        let openAction = AlertPresentableAction(title: String(localized: .ChatExtension.linkOpenAction)) {
            onOpen()
        }

        let viewModel = AlertPresentableViewModel(
            title: String(localized: .ChatExtension.linkOpenTitle),
            message: url.absoluteString,
            actions: [openAction],
            closeActionTitle: String(localized: .Common.cancel)
        )

        present(viewModel: viewModel, style: .alert, from: view)
    }

    func openProductLink(_ url: URL, from view: ControllerBackedProtocol?) {
        ChatProductLinkOpener(linkHandler: DeferredLinkHandler.shared).open(url) { [weak self, weak view] webURL in
            guard let view else { return }
            self?.showWeb(url: webURL, from: view, style: WebPresentableStyle(mode: .modal(.fullScreen)))
        }
    }
}

/// Picks the way to open a link of a product action.
struct ChatProductLinkOpener {
    enum Route: Equatable {
        /// The link pipeline of the app takes it: a product page, a room, a Pocket card.
        case app
        case web
        case system
    }

    let linkHandler: DeferredLinkHandling

    static func route(for url: URL) -> Route {
        switch url.scheme?.lowercased() {
        case "polkadot": .app
        case "http",
             "https": .web
        default: .system
        }
    }

    @MainActor
    func open(_ url: URL, showWeb: (URL) -> Void) {
        switch Self.route(for: url) {
        case .app:
            linkHandler.handle(with: url)
        case .web:
            showWeb(url)
        case .system:
            UIApplication.shared.open(url)
        }
    }
}
