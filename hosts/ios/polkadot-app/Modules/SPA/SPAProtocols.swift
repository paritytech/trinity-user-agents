import Foundation
import Products
import UIKit
import UIKitExt

@MainActor
protocol SPAViewProtocol: ControllerBackedProtocol {
    /// The page's own scroll view, which a screen around it can hang a header
    /// in so the header rides the page rather than pinning above it.
    var pageScrollView: UIScrollView { get }

    func navigate(to url: URL)
    func navigate(to page: ProductPage)
    func updateTitle(_ title: String)
    func reload()
    func showLoading()
    func hideLoading()
    func showLoadFailure(_ content: ErrorContent)
    func updateLoadProgress(_ progress: DotNsLoadProgress)
}

@MainActor
protocol SPAPresenterProtocol: AnyObject {
    func setup(engine: JSEngineProtocol)
    func didTapMoreButton()
    func didTapMinimize()
    func didTapClose()
    func didTapRetry()
    func didInterceptNavigation(to url: URL)
    func didUpdateWebViewTitle(_ title: String)
    func hasChatEntry() -> Bool
    func didTapOpenChat()
    func didTapShare()
}

@MainActor
protocol SPAInteractorInputProtocol: AnyObject {
    func setup(engine: JSEngineProtocol)
    func retry()
    func hasChatEntry() -> Bool
    func openChat()
}

@MainActor
protocol SPAInteractorOutputProtocol: AnyObject {
    func didFail(error: Error)
    func didRequestNavigation(to url: URL)
    func didPrepareChat(chatId: Chat.Id)
    func didUpdateLoadProgress(_ progress: DotNsLoadProgress)
}

@MainActor
protocol SPAWireframeProtocol: AlertPresentable, ErrorPresentable {
    func showProductSPA(from view: ControllerBackedProtocol?, productHost: ProductHost)
    func showMoreActions(
        from view: ControllerBackedProtocol?,
        actions: [SPAMoreAction],
        closeTitle: String
    )
    func shareURL(_ url: URL, from view: ControllerBackedProtocol?)

    func openChat(
        from view: ControllerBackedProtocol?,
        chatId: Chat.Id
    )

    func minimize()
    func close(tabId: UUID)
}
