import UIKit
import UIKitExt
import Foundation_iOS
import PolkadotUI

@MainActor
final class TransferAmountWireframe: TransferAmountWireframeProtocol {
    let recipient: RecipientModel

    init(recipient: RecipientModel) {
        self.recipient = recipient
    }

    func hide(view: ControllerBackedProtocol?) {
        let navigationController = view?.controller.navigationController

        navigateToChat(with: .person(recipient.accountId), force: false)
        navigationController?.popToRootViewController(animated: false)
    }
}
