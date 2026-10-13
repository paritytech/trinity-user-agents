import PolkadotUI
import SnapKit
import UIKit

/// Owns the floating widget stack under the bar: the container, its bottom constraint, the
/// attached widget controllers and the bottom safe-area inset those widgets take from the screen
/// under them. The host runs its own layout pass; this reports the height that pass needs and
/// calls back when the content changes.
@MainActor
final class TabBarWidgetController {
    private let containerView = MainTabBarFloatingWidgetStackView()
    private let onContentChanged: () -> Void

    private var controllers: [AppWidgetID: AppWidgetContentViewController] = [:]
    private var bottomConstraint: Constraint?
    private var isInstalled = false
    private weak var appliedTabController: UIViewController?
    private weak var appliedContentController: UIViewController?
    private weak var contentSafeAreaAdjustedViewController: UIViewController?

    init(onContentChanged: @escaping () -> Void) {
        self.onContentChanged = onContentChanged
    }

    var hasWidgets: Bool {
        !controllers.isEmpty
    }

    /// The container goes below `sibling` so the bar's glass draws over the widgets. Widgets
    /// attached before the host loaded its view are installed here, not on attach.
    func install(in view: UIView, below sibling: UIView) {
        containerView.translatesAutoresizingMaskIntoConstraints = false
        view.insertSubview(containerView, belowSubview: sibling)

        containerView.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview()
            bottomConstraint = make.bottom.equalToSuperview().constraint
        }

        isInstalled = true
        controllers.values.forEach(install)
    }

    func attach(_ configuration: any HashableContentConfiguration, for id: AppWidgetID) {
        if let controller = controllers[id] {
            controller.update(configuration: configuration)
            onContentChanged()
            return
        }

        let controller = AppWidgetContentViewController(configuration: configuration)
        controllers[id] = controller
        install(controller)
    }

    func detach(for id: AppWidgetID) {
        guard let controller = controllers.removeValue(forKey: id) else {
            return
        }

        containerView.removeArrangedSubview(controller.view)
        controller.view.removeFromSuperview()

        onContentChanged()
    }

    func setBottomOffset(_ offset: CGFloat) {
        bottomConstraint?.update(offset: offset)
    }

    func setAppliedControllers(tab: UIViewController?, content: UIViewController?) {
        appliedTabController = tab
        appliedContentController = content
    }

    /// The bar clearance belongs to the tab, the widget height to the screen the widgets float
    /// over. They land on the same controller only when the tab shows no navigation stack.
    func updateContentSafeAreaInset(barClearance: CGFloat, fittingWidth: CGFloat) {
        let tabController = appliedTabController

        guard let contentController = appliedContentController else {
            releaseWidgetInset(keeping: nil)
            tabController?.additionalSafeAreaInsets.bottom = barClearance
            return
        }

        releaseWidgetInset(keeping: contentController)

        let widgetInset = contentHeight(fittingWidth: fittingWidth)

        if contentController === tabController {
            contentController.additionalSafeAreaInsets.bottom = barClearance + widgetInset
        } else {
            tabController?.additionalSafeAreaInsets.bottom = barClearance
            contentController.additionalSafeAreaInsets.bottom = widgetInset
        }

        contentSafeAreaAdjustedViewController = widgetInset > 0 ? contentController : nil
    }

    func contentHeight(fittingWidth: CGFloat) -> CGFloat {
        guard hasWidgets else {
            return 0
        }

        let width = max(containerView.bounds.width, fittingWidth)
        let fittingSize = CGSize(
            width: width,
            height: UIView.layoutFittingCompressedSize.height
        )
        let measuredSize = containerView.systemLayoutSizeFitting(
            fittingSize,
            withHorizontalFittingPriority: .required,
            verticalFittingPriority: .fittingSizeLevel
        )

        return max(containerView.bounds.height, measuredSize.height)
    }
}

private extension TabBarWidgetController {
    func install(_ controller: UIViewController) {
        guard isInstalled else {
            return
        }

        controller.loadViewIfNeeded()
        containerView.addArrangedSubview(controller.view)

        onContentChanged()
    }

    /// A screen that was inset for the widgets keeps that inset only while it is still the one
    /// showing; once another screen takes over, the old one is handed its full height back.
    func releaseWidgetInset(keeping contentController: UIViewController?) {
        guard contentSafeAreaAdjustedViewController !== contentController else {
            return
        }

        contentSafeAreaAdjustedViewController?.additionalSafeAreaInsets.bottom = 0
        contentSafeAreaAdjustedViewController = nil
    }
}
