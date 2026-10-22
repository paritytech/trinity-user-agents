import UIKit

// MARK: - Floating widget layout

extension MainTabBarViewController {
    func applyWidgetLayout(
        for context: TabBarChromeContext,
        animatingAlongside transitionCoordinator: UIViewControllerTransitionCoordinator? = nil
    ) {
        widgetController.setAppliedControllers(tab: context.tabController, content: context.contentController)

        updateWidgetLayout(animatingAlongside: transitionCoordinator)
    }

    func updateWidgetLayout(animatingAlongside transitionCoordinator: UIViewControllerTransitionCoordinator? = nil) {
        guard view.window != nil else {
            return
        }

        widgetController.setBottomOffset(-chromeController.restingBarHeight)
        widgetController.updateContentSafeAreaInset(
            barClearance: chromeController.restingBottomClearance,
            fittingWidth: view.bounds.width
        )

        guard widgetController.hasWidgets else {
            return
        }

        animateWidgetConstraintChange(with: transitionCoordinator)
    }
}

private extension MainTabBarViewController {
    func animateWidgetConstraintChange(with transitionCoordinator: UIViewControllerTransitionCoordinator?) {
        view.setNeedsLayout()

        guard let transitionCoordinator else {
            view.layoutIfNeeded()
            return
        }

        transitionCoordinator.animate(
            alongsideTransition: { [weak self] _ in
                self?.view.layoutIfNeeded()
            }
        )
    }
}
