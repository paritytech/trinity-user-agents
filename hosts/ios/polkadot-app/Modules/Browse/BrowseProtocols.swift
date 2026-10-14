import Foundation
import Products
import UIKitExt

@MainActor
protocol BrowseViewProtocol: ControllerBackedProtocol {
    func showLoading()
    func hideLoading()
}

@MainActor
protocol BrowsePresenterProtocol: AnyObject {
    func setup()
}

protocol BrowseInteractorInputProtocol: AnyObject {
    func resolveBrowseHost()
}

@MainActor
protocol BrowseInteractorOutputProtocol: AnyObject {
    func didResolve(host: ProductHost)
    func didFailResolving()
}

@MainActor
protocol BrowseWireframeProtocol: CommonRetryable, AlertPresentable {
    /// Swaps the SPA view in as the navigation root.
    /// Returns false when the SPA view could not be assembled, so the presenter can show retry.
    @discardableResult
    func showSPA(from view: BrowseViewProtocol?, host: ProductHost) -> Bool
}
