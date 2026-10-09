import UIKit

// MARK: - Floating widget layout

extension MainTabBarViewController {
    func applyWidgetLayout(
        for context: TabBarChromeContext,
        animatingAlongside transitionCoordinator: UIViewControllerTransitionCoordinator? = nil
    ) {
        appliedTabController = context.tabController
        appliedContentController = context.contentController

        updateWidgetLayout(animatingAlongside: transitionCoordinator)
    }

    func updateWidgetLayout(animatingAlongside transitionCoordinator: UIViewControllerTransitionCoordinator? = nil) {
        guard view.window != nil else {
            return
        }

        widgetController.setBottomOffset(-chromeController.restingBarHeight)
        updateContentSafeAreaInset()

        guard widgetController.hasWidgets else {
            return
        }

        animateWidgetConstraintChange(with: transitionCoordinator)
    }

    /// The bar clearance belongs to the tab, the widget height to the screen the widgets float
    /// over. They land on the same controller only when the tab shows no navigation stack.
    private func updateContentSafeAreaInset() {
        let tabController = appliedTabController
        let barClearance = chromeController.restingBottomClearance

        guard let contentController = appliedContentController else {
            releaseWidgetInset(keeping: nil)
            tabController?.additionalSafeAreaInsets.bottom = barClearance
            return
        }

        releaseWidgetInset(keeping: contentController)

        let widgetInset = widgetController.contentHeight(fittingWidth: view.bounds.width)

        if contentController === tabController {
            contentController.additionalSafeAreaInsets.bottom = barClearance + widgetInset
        } else {
            tabController?.additionalSafeAreaInsets.bottom = barClearance
            contentController.additionalSafeAreaInsets.bottom = widgetInset
        }

        contentSafeAreaAdjustedViewController = widgetInset > 0 ? contentController : nil
    }

    /// A screen that was inset for the widgets keeps that inset only while it is still the one
    /// showing; once another screen takes over, the old one is handed its full height back.
    private func releaseWidgetInset(keeping contentController: UIViewController?) {
        guard contentSafeAreaAdjustedViewController !== contentController else {
            return
        }

        contentSafeAreaAdjustedViewController?.additionalSafeAreaInsets.bottom = 0
        contentSafeAreaAdjustedViewController = nil
    }

    private func animateWidgetConstraintChange(with transitionCoordinator: UIViewControllerTransitionCoordinator?) {
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
