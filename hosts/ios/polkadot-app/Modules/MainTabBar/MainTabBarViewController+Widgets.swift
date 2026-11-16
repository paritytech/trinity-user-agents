import UIKit
import PolkadotUI

// MARK: - Floating widget layout

extension MainTabBarViewController {
    func attachWidget(_ configuration: any HashableContentConfiguration, for id: AppWidgetID) {
        widgetController.attach(configuration, for: id)
    }

    func detachWidget(for id: AppWidgetID) {
        widgetController.detach(for: id)
    }

    func applyWidgetLayout(
        for context: TabBarChromeContext,
        animatingAlongside transitionCoordinator: UIViewControllerTransitionCoordinator? = nil
    ) {
        widgetController.setAppliedControllers(tab: context.tabController, content: context.contentController)

        updateWidgetLayout(for: context, animatingAlongside: transitionCoordinator)
    }

    func updateWidgetLayout(
        for context: TabBarChromeContext,
        animatingAlongside transitionCoordinator: UIViewControllerTransitionCoordinator? = nil
    ) {
        guard view.window != nil else {
            return
        }

        widgetController.setBottomOffset(-chromeController.restingBarHeight(for: context))
        widgetController.updateContentSafeAreaInset(
            barClearance: chromeController.bottomClearance(for: context),
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
