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
        mountController(at: index, animated: false)
    }

    func select(index: Int) {
        guard controllers.indices.contains(index), index != currentIndex || selection.isSPA else {
            return
        }
        currentIndex = index
        selection = .tab(index)
        mountController(at: index, animated: true)
    }

    func mountSPA(_ controller: UIViewController, for tabId: UUID) {
        selection = .spa(tabId)
        mount(controller, animated: true)
    }

    func unmountSPA() {
        guard selection.isSPA else {
            return
        }
        selection = .tab(currentIndex)
        mountController(at: currentIndex, animated: true)
    }

    func controller(at index: Int) -> UIViewController? {
        controllers.indices.contains(index) ? controllers[index] : nil
    }
}

private extension TabBarContainer {
    var hostView: UIView? {
        hostController?.view
    }

    func mountController(at index: Int, animated: Bool) {
        guard controllers.indices.contains(index) else {
            return
        }
        mount(controllers[index], animated: animated)
    }

    func mount(_ incoming: UIViewController, animated: Bool) {
        guard
            let hostController,
            let hostView,
            incoming !== selectedController
        else {
            return
        }

        let outgoing = selectedController
        outgoing?.willMove(toParent: nil)

        hostController.addChild(incoming)
        incoming.view.frame = hostView.bounds
        incoming.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        hostView.insertSubview(incoming.view, at: 0)
        incoming.didMove(toParent: hostController)

        selectedController = incoming
        onContentInsetInvalidated?()

        guard animated else {
            finishUnmount(outgoing)
            return
        }

        incoming.view.alpha = 0
        incoming.view.transform = CGAffineTransform(scaleX: 0.998, y: 0.998)
        UIView.animate(
            withDuration: 0.15,
            animations: {
                incoming.view.alpha = 1
                incoming.view.transform = .identity
                outgoing?.view.alpha = 0
            },
            completion: { _ in
                self.finishUnmount(outgoing)
            }
        )
    }

    func finishUnmount(_ controller: UIViewController?) {
        guard let controller, controller !== selectedController else {
            return
        }
        controller.view.removeFromSuperview()
        controller.removeFromParent()
        controller.view.alpha = 1
    }
}
