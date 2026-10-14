import PolkadotUI
import UIKit
import UIKitExt

/// What the room screen shows for the parts of a product room that its product declares (P6).
@MainActor
protocol ChatProductRoomViewProtocol: ControllerBackedProtocol {
    /// The buttons of the room toolbar. The first item is the trailing item.
    func didReceive(productToolbar: [UIBarButtonItem])
    func showReactionDetails(viewModel: ReactionDetailsViewModel)
    /// Shows why an action of the user did not happen.
    func showActionFailure(_ message: String)
    /// Scrolls to the message now, or when it arrives in the window.
    func scrollToMessage(_ messageId: String)
}

extension ChatViewController: ChatProductRoomViewProtocol {
    func didReceive(productToolbar: [UIBarButtonItem]) {
        navigationBarController.apply(productToolbar: productToolbar)
    }
}
