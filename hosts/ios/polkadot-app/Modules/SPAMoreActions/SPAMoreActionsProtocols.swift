import UIKit
import UIKitExt

struct SPAMoreAction {
    let icon: UIImage?
    let title: String
    let isEnabled: Bool
    let handler: () -> Void
}

protocol SPAMoreActionsViewProtocol: ControllerBackedProtocol {}

@MainActor
protocol SPAMoreActionsPresenterProtocol: AnyObject {
    var actions: [SPAMoreAction] { get }
    var closeTitle: String { get }
    func didSelectAction(at index: Int)
    func didSelectClose()
}
