import UIKit

// MARK: - Navigation transitions

extension MainTabBarViewController {
    func chromeContext(
        tabController: UIViewController,
        stack: [UIViewController],
        showing target: UIViewController
    ) -> TabBarChromeContext {
        TabBarChromeContext(
            tabController: tabController,
            contentController: target,
            isTabRoot: TabBarHiddenPolicy.isTabRoot(in: stack, showing: target),
            foldDerived: TabBarHiddenPolicy.deriveFoldState(in: stack, showing: target),
            screen: target
        )
    }
}

private extension MainTabBarViewController {
    func stayingChromeContext(
        in navigationController: UINavigationController,
        context: UIViewControllerTransitionCoordinatorContext
    ) -> TabBarChromeContext? {
        guard let staying = context.viewController(forKey: .from) else {
            return nil
        }
        let restored = TabBarHiddenPolicy.stackAfterCancelledPop(
            stack: navigationController.viewControllers,
            staying: staying
        )
        return chromeContext(tabController: navigationController, stack: restored, showing: staying)
    }

    func track(
        of navigationController: AppNavigationController,
        showing target: UIViewController,
        animated: Bool
    ) {
        let incoming = chromeContext(
            tabController: navigationController,
            stack: navigationController.viewControllers,
            showing: target
        )

        guard
            animated,
            let coordinator = navigationController.transitionCoordinator,
            coordinator.isInteractive
        else {
            applyChrome(
                incoming,
                animatingAlongside: animated ? navigationController.transitionCoordinator : nil
            )
            return
        }

        applyWidgetLayout(for: incoming, animatingAlongside: coordinator)

        let targetState = chromeController.resolvedState(for: incoming)
        coordinator.animate(alongsideTransition: { [weak self] _ in
            self?.chromeController.setInteractiveTarget(targetState)
        })

        coordinator.notifyWhenInteractionChanges { [weak self] context in
            guard let self else {
                return
            }
            let staying = context.isCancelled
                ? stayingChromeContext(in: navigationController, context: context)
                : nil
            applyChrome(staying ?? incoming)
        }
    }
}

// MARK: - AppNavigationControllerTransitionObserving

extension MainTabBarViewController: AppNavigationControllerTransitionObserving {
    func appNavigationController(
        _ navigationController: AppNavigationController,
        willShow viewController: UIViewController,
        animated: Bool
    ) {
        track(of: navigationController, showing: viewController, animated: animated)
    }

    func appNavigationController(
        _ navigationController: AppNavigationController,
        didShow viewController: UIViewController,
        animated _: Bool
    ) {
        applyChrome(chromeContext(
            tabController: navigationController,
            stack: navigationController.viewControllers,
            showing: viewController
        ))
    }
}
