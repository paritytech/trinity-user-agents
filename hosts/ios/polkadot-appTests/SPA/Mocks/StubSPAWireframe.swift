import Foundation
import Products
import UIKit
import UIKitExt
@testable import polkadot_app

/// Discards every navigation, since the Refresh path never leaves the page. Alerts are kept so a test can read
/// them and fire their actions.
@MainActor
final class StubSPAWireframe: SPAWireframeProtocol {
    private(set) var presentedAlerts: [AlertPresentableViewModel] = []
    let alertPresented = TestSignal()

    func present(
        message _: String?,
        title _: String?,
        closeAction _: String?,
        from _: ControllerBackedProtocol?
    ) {}

    func present(
        viewModel: AlertPresentableViewModel,
        style _: UIAlertController.Style,
        from _: ControllerBackedProtocol?
    ) {
        presentedAlerts.append(viewModel)
        alertPresented.fire()
    }

    func present(error _: ErrorContent, from _: ControllerBackedProtocol?) -> Bool {
        false
    }

    func showProductSPA(from _: ControllerBackedProtocol?, productHost _: ProductHost) {}

    func openDeeplink(_: URL) {}

    func showMoreActions(
        from _: ControllerBackedProtocol?,
        actions _: [SPAMoreAction],
        closeTitle _: String
    ) {}

    func shareURL(_: URL, from _: ControllerBackedProtocol?) {}

    func openChat(from _: ControllerBackedProtocol?, chatId _: Chat.Id) {}

    func minimize() {}

    func close(tabId _: UUID) {}
}
