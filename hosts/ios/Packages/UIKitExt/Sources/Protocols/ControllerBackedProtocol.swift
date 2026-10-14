import UIKit

@MainActor public protocol ControllerBackedProtocol: AnyObject {
    var isSetup: Bool { get }
    var controller: UIViewController { get }
}

public extension ControllerBackedProtocol where Self: UIViewController {
    var isSetup: Bool {
        controller.isViewLoaded
    }

    var controller: UIViewController {
        self
    }
}

public final class ControllerBackedWrapper {
    public let controller: UIViewController

    init(controller: UIViewController) {
        self.controller = controller
    }
}

extension ControllerBackedWrapper: ControllerBackedProtocol {
    public var isSetup: Bool {
        controller.isViewLoaded
    }
}
