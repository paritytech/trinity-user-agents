import UIKit

@MainActor
final class TabBarContainer {
    private weak var hostController: UIViewController?

    private var controllers: [UIViewController] = []
    private var currentIndex = 0

    private(set) var selection: TabBarContentSelection = .tab(0)
    private(set) var selectedController: UIViewController?

    var onContentInsetInvalidated: (() -> Void)?

    init(hostController: UIViewController) {
        self.hostController = hostController
    }

    func setControllers(_ controllers: [UIViewController], selecting index: Int) {
        self.controllers = controllers
        currentIndex = index
        selection = .tab(index)
        mountController(at: index)
    }

    func select(index: Int) {
        guard controllers.indices.contains(index), index != currentIndex || selection.isSPA else {
            return
        }
        currentIndex = index
        selection = .tab(index)
        mountController(at: index)
    }

    func mountSPA(_ controller: UIViewController, for tabId: UUID) {
        selection = .spa(tabId)
        mount(controller)
    }

    func unmountSPA() {
        guard selection.isSPA else {
            return
        }
        selection = .tab(currentIndex)
        mountController(at: currentIndex)
    }

    func controller(at index: Int) -> UIViewController? {
        controllers.indices.contains(index) ? controllers[index] : nil
    }
}

private extension TabBarContainer {
    var hostView: UIView? {
        hostController?.view
    }

    func mountController(at index: Int) {
        guard controllers.indices.contains(index) else {
            return
        }
        mount(controllers[index])
    }

    func mount(_ incoming: UIViewController) {
        guard
            let hostController,
            let hostView,
            incoming !== selectedController
        else {
            return
        }

        let outgoing = selectedController
        selectedController = incoming

        outgoing?.willMove(toParent: nil)
        outgoing?.view.removeFromSuperview()
        outgoing?.removeFromParent()

        hostController.addChild(incoming)
        incoming.view.frame = hostView.bounds
        incoming.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        hostView.insertSubview(incoming.view, at: 0)
        incoming.didMove(toParent: hostController)

        onContentInsetInvalidated?()
    }
}
