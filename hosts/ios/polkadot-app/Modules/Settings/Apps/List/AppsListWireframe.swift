import Foundation
import Products

@MainActor
final class AppsListWireframe: AppsListWireframeProtocol {
    private let flowStateProvider: any SPAFlowStateProviding

    init(flowStateProvider: any SPAFlowStateProviding) {
        self.flowStateProvider = flowStateProvider
    }

    func showAppDetail(productId: ProductId, from view: AppsListViewProtocol?) {
        guard let detailView = AppDetailViewFactory.createView(
            productId: productId,
            flowStateProvider: flowStateProvider
        ) else {
            return
        }

        view?.controller.navigationController?.pushViewController(
            detailView.controller,
            animated: true
        )
    }
}
