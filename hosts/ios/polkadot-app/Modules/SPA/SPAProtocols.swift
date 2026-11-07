import Foundation
import PolkadotUI
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
    func updateLoadPhase(_ phase: ProductLoadPhase)
    func showChainStatus(_ rows: [ChainConnectionStatusViewModel])
}

extension SPAViewProtocol {
    func showChainStatus(_: [ChainConnectionStatusViewModel]) {}
}

@MainActor
protocol SPAPresenterProtocol: AnyObject {
    func setup(engine: JSEngineProtocol)
    func didTapMoreButton()
    func didTapClose()
    func didTapRefresh()
    func didBecomeVisible()
    func didInterceptNavigation(to url: URL)
    func didRequestDeeplink(_ url: URL)
    func didUpdateWebViewTitle(_ title: String)
    func didFinishPageLoad(loaded: Bool)
    func hasChatEntry() -> Bool
    func didTapOpenChat()
    func didTapShare()
}

@MainActor
protocol SPAInteractorInputProtocol: AnyObject {
    func setup(engine: JSEngineProtocol)
    func retry()
    func reloadLatestBuild()
    func checkForNewBuild()
    func hasChatEntry() -> Bool
    func openChat()
    func didFinishPageLoad(loaded: Bool)
}

@MainActor
protocol SPAInteractorOutputProtocol: AnyObject {
    func didFail(error: Error)
    func didRequestNavigation(to url: URL)
    func didRequestReload()
    func didRestartLoading()
    func didPrepareChat(chatId: Chat.Id)
    func didUpdateLoadPhase(_ phase: ProductLoadPhase)
    func didReceiveChainStatus(_ rows: [ChainConnectionStatusViewModel])
}

@MainActor
protocol SPAWireframeProtocol: AlertPresentable, ErrorPresentable {
    func showProductSPA(from view: ControllerBackedProtocol?, productHost: ProductHost)
    func openDeeplink(_ url: URL)
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

    func close(tabId: UUID)
    func dismiss(view: ControllerBackedProtocol?)
}
