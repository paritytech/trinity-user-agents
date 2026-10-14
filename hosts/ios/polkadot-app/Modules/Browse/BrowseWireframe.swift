import UIKit
import Products
import UIKitExt

@MainActor
final class BrowseWireframe: BrowseWireframeProtocol, CommonRetryable, AlertPresentable {
    private let flowStateProvider: SPAFlowStateProviding

    init(flowStateProvider: SPAFlowStateProviding) {
        self.flowStateProvider = flowStateProvider
    }

    func showSPA(from view: BrowseViewProtocol?, host: ProductHost) -> Bool {
        let flowState = flowStateProvider.flowState()

        let configuration = SPAConfiguration.browseRoot(host: host)

        guard let spaView = SPAViewFactory.createView(
            configuration: configuration,
            flowState: flowState
        ) else { return false }

        view?.controller.navigationController?.setViewControllers(
            [spaView.controller],
            animated: false
        )

        return true
    }
}
