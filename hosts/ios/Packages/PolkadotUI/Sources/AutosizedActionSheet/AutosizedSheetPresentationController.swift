import UIKit
internal import UIKit_iOS

final class AutosizedSheetPresentationController: UIPresentationController {
    let configuration: ModalSheetPresentationConfiguration

    private var backgroundView: UIView?
    private var headerView: RoundedView?
    private weak var originalNavigationDelegate: UINavigationControllerDelegate?
    private var navigationDelegateProxy: NavigationDelegateProxy?

    var interactiveDismissal: UIPercentDrivenInteractiveTransition?
    var initialTranslation: CGPoint = .zero

    var presenterDelegate: ModalPresenterDelegate? {
        (presentedViewController as? ModalPresenterDelegate) ??
            (presentedView as? ModalPresenterDelegate) ??
            (presentedViewController.view as? ModalPresenterDelegate)
    }

    var sheetPresenterDelegate: ModalSheetPresenterDelegate? {
        presenterDelegate as? ModalSheetPresenterDelegate
    }

    var inputView: ModalViewProtocol? {
        (presentedViewController as? ModalViewProtocol) ??
            (presentedView as? ModalViewProtocol) ??
            (presentedViewController.view as? ModalViewProtocol)
    }

    init(
        presentedViewController: UIViewController,
        presenting presentingViewController: UIViewController?,
        configuration: ModalSheetPresentationConfiguration
    ) {
        self.configuration = configuration

        super.init(presentedViewController: presentedViewController, presenting: presentingViewController)

        if let modalInputView = inputView {
            modalInputView.presenter = self
        }
    }

    // MARK: Presentation overridings

    override func presentationTransitionWillBegin() {
        guard let containerView else {
            return
        }

        configureBackgroundView(on: containerView)

        if let headerStyle = configuration.style.headerStyle {
            configureHeaderView(on: presentedViewController.view, style: headerStyle)
        }

        attachCancellationGesture()
        attachPanGesture()
        animateBackgroundAlpha(fromValue: 0.0, toValue: 1.0)
    }

    override func presentationTransitionDidEnd(_ completed: Bool) {
        super.presentationTransitionDidEnd(completed)
        installNavigationDelegateProxyIfNeeded()
    }

    override func dismissalTransitionWillBegin() {
        animateBackgroundAlpha(fromValue: 1.0, toValue: 0.0)
    }

    override func containerViewWillLayoutSubviews() {
        super.containerViewWillLayoutSubviews()
        let frameInContainer = frameOfPresentedViewInContainerView
        presentedView?.frame = frameInContainer
        guard let presentedView else { return }
        updateHeaderFrame(for: frameInContainer, presentedView: presentedView)
    }

    override func containerViewDidLayoutSubviews() {
        super.containerViewDidLayoutSubviews()
        applyTopAlignedChildLayoutIfNeeded()
    }

    override func preferredContentSizeDidChange(forChildContentContainer container: UIContentContainer) {
        super.preferredContentSizeDidChange(forChildContentContainer: container)

        containerView?.setNeedsLayout()
        configuration.contentSizeAnimator.animate(block: {
            self.containerView?.layoutIfNeeded()
        }, completionBlock: nil)
    }

    override var frameOfPresentedViewInContainerView: CGRect {
        guard let containerView else { return .zero }

        let presented = (presentedViewController as? UINavigationController)?
            .topViewController ?? presentedViewController

        var frame = containerView.bounds
        switch configuration.style.sizing {
        case .manual:
            let (layoutFrame, bottomOffset, maximumHeight) = calculateLayoutParameters(
                containerView: containerView,
                extendUnderSafeArea: configuration.extendUnderSafeArea
            )

            let preferredSize = presented.preferredContentSize
            let layoutWidth = preferredSize.width > 0.0 ? preferredSize.width : layoutFrame.width
            let layoutHeight = preferredSize.height > 0.0
                ? preferredSize.height + bottomOffset
                : layoutFrame.height

            frame.size.height = min(layoutHeight, maximumHeight)
            frame.size.width = layoutWidth
            frame.origin.y = layoutFrame.maxY - frame.size.height
        case let .auto(maxHeight):
            let targetSize = CGSize(
                width: containerView.bounds.width,
                height: UIView.layoutFittingCompressedSize.height
            )

            let fittingSize = presented.view.systemLayoutSizeFitting(
                targetSize,
                withHorizontalFittingPriority: .required,
                verticalFittingPriority: .defaultLow
            )

            let (layoutFrame, bottomOffset, maximumHeight) = calculateLayoutParameters(
                containerView: containerView,
                extendUnderSafeArea: configuration.extendUnderSafeArea
            )

            let maxHeightInContainer = min(maxHeight, 1.0) * maximumHeight
            let contentHeight = max(
                fittingSize.height,
                presented.preferredContentSize.height
            )
            frame.size.height = min(contentHeight + bottomOffset, maxHeightInContainer)
            frame.origin.y = layoutFrame.maxY - frame.size.height
        }

        return frame
    }
}

private extension AutosizedSheetPresentationController {
    func resolvePreferredSize(for viewController: UIViewController) -> CGSize {
        if viewController.preferredContentSize != .zero {
            return viewController.preferredContentSize
        }

        viewController.view.layoutIfNeeded()

        let targetWidth = containerView?.bounds.width
            ?? viewController.view.window?.bounds.width
            ?? UIScreen.main.bounds.width
        let fitting = viewController.view.systemLayoutSizeFitting(
            CGSize(width: targetWidth, height: UIView.layoutFittingCompressedSize.height),
            withHorizontalFittingPriority: .required,
            verticalFittingPriority: .defaultLow
        )

        if fitting.height > 0 { return fitting }

        if let scrollView = viewController.view.firstDescendantScrollView {
            let window = viewController.view.window
            let screenHeight = window?.bounds.height ?? UIScreen.main.bounds.height
            let safeArea = window?.safeAreaInsets ?? .zero
            let maxHeight = screenHeight - safeArea.top - safeArea.bottom
            return CGSize(width: 0, height: min(scrollView.contentSize.height, maxHeight))
        }

        return fitting
    }

    func handleNavigationTransition(animated: Bool) {
        let apply = { [weak self] in
            self?.applyTopAlignedChildLayoutIfNeeded()
        }

        if animated, let coordinator = presentedViewController.transitionCoordinator {
            coordinator.animate(alongsideTransition: { _ in apply() }, completion: { _ in apply() })
        } else {
            apply()
        }
    }
}

extension AutosizedSheetPresentationController {
    private func configureBackgroundView(on view: UIView) {
        if let currentBackgroundView = backgroundView {
            view.insertSubview(currentBackgroundView, at: 0)
        } else {
            let newBackgroundView = UIView(frame: view.bounds)

            newBackgroundView.backgroundColor = configuration.style.backdropColor

            view.insertSubview(newBackgroundView, at: 0)
            backgroundView = newBackgroundView
        }

        backgroundView?.frame = view.bounds
    }

    private func configureHeaderView(on view: UIView, style: ModalSheetPresentationHeaderStyle) {
        let width = containerView?.bounds.width ?? view.bounds.width

        if let headerView {
            view.insertSubview(headerView, at: 0)
        } else {
            let baseView = RoundedView()
            baseView.cornerRadius = style.cornerRadius
            baseView.roundingCorners = [.topLeft, .topRight]
            baseView.fillColor = style.backgroundColor
            baseView.highlightedFillColor = style.backgroundColor
            baseView.shadowOpacity = 0.0

            let indicator = RoundedView()
            indicator.roundingCorners = .allCorners
            indicator.cornerRadius = style.indicatorSize.height / 2.0
            indicator.fillColor = style.indicatorColor
            indicator.highlightedFillColor = style.indicatorColor
            indicator.shadowOpacity = 0.0

            baseView.addSubview(indicator)

            let indicatorX = width / 2.0 - style.indicatorSize.width / 2.0
            indicator.frame = CGRect(
                origin: CGPoint(x: indicatorX, y: style.indicatorVerticalOffset),
                size: style.indicatorSize
            )

            view.insertSubview(baseView, at: 0)

            headerView = baseView
        }

        headerView?.frame = CGRect(
            x: 0.0,
            y: -style.preferredHeight + 0.5,
            width: width,
            height: style.preferredHeight
        )
    }

    private func attachCancellationGesture() {
        let cancellationGesture = UITapGestureRecognizer(
            target: self,
            action: #selector(actionDidCancel(gesture:))
        )
        backgroundView?.addGestureRecognizer(cancellationGesture)
    }

    private func attachPanGesture() {
        let panGestureRecognizer = UIPanGestureRecognizer(target: self, action: #selector(didPan(sender:)))
        containerView?.addGestureRecognizer(panGestureRecognizer)
        panGestureRecognizer.delegate = self
    }

    private func installNavigationDelegateProxyIfNeeded() {
        guard let navigationController = presentedViewController as? UINavigationController else {
            return
        }

        let proxy = NavigationDelegateProxy(presentationController: self)
        originalNavigationDelegate = navigationController.delegate
        proxy.originalDelegate = navigationController.delegate
        navigationController.delegate = proxy
        navigationDelegateProxy = proxy
    }

    // Aligns navigation controller so that only bottom part is clipped if height is larger than the container
    private func applyTopAlignedChildLayoutIfNeeded() {
        guard let navigationController = presentedViewController as? UINavigationController,
              let topVC = navigationController.topViewController else {
            return
        }

        let navBounds = navigationController.view.bounds
        let navBarMaxY = navigationController.navigationBar.isHidden
            ? 0.0
            : navigationController.navigationBar.frame.maxY
        let contentTop = navBarMaxY
        let contentWidth = navBounds.width
        let availableHeight = navBounds.height - contentTop

        guard contentWidth > 0 else { return }

        let fittingSize = topVC.view.systemLayoutSizeFitting(
            CGSize(width: contentWidth, height: UIView.layoutFittingCompressedSize.height),
            withHorizontalFittingPriority: .required,
            verticalFittingPriority: .defaultLow
        )
        let preferredHeight = max(topVC.preferredContentSize.height, fittingSize.height)

        guard preferredHeight > availableHeight else {
            return
        }

        let targetFrame = CGRect(
            x: 0.0,
            y: contentTop,
            width: contentWidth,
            height: preferredHeight
        )

        guard topVC.view.frame != targetFrame else {
            return
        }
        topVC.view.autoresizingMask = [.flexibleWidth]
        topVC.view.frame = targetFrame
        topVC.view.layoutIfNeeded()
    }

    private func updateHeaderFrame(for frame: CGRect, presentedView: UIView) {
        guard let headerStyle = configuration.style.headerStyle else { return }
        let width = containerView?.bounds.width ?? presentedView.bounds.width
        headerView?.frame = CGRect(
            x: 0.0,
            y: frame.origin.y - headerStyle.preferredHeight + 0.5,
            width: width,
            height: headerStyle.preferredHeight
        )
    }

    private func calculateLayoutParameters(
        containerView: UIView,
        extendUnderSafeArea: Bool
    ) -> (layoutFrame: CGRect, bottomOffset: CGFloat, maximumHeight: CGFloat) {
        let layoutFrame: CGRect
        let bottomOffset: CGFloat
        let maximumHeight: CGFloat

        if extendUnderSafeArea {
            layoutFrame = containerView.bounds
            bottomOffset = containerView.safeAreaInsets.bottom
            maximumHeight = layoutFrame.size.height - containerView.safeAreaInsets.top
        } else {
            layoutFrame = containerView.safeAreaLayoutGuide.layoutFrame
            bottomOffset = 0.0
            maximumHeight = layoutFrame.size.height
        }

        return (layoutFrame, bottomOffset, maximumHeight - bottomOffset)
    }

    // MARK: Animation

    func animateBackgroundAlpha(fromValue: CGFloat, toValue: CGFloat) {
        backgroundView?.alpha = fromValue

        let animationBlock: (UIViewControllerTransitionCoordinatorContext) -> Void = { _ in
            self.backgroundView?.alpha = toValue
        }

        presentingViewController.transitionCoordinator?
            .animate(alongsideTransition: animationBlock, completion: nil)
    }

    func dismiss(animated: Bool, completion: (() -> Void)?) {
        presentedViewController.dismiss(animated: animated, completion: completion)
    }

    // MARK: Action

    @objc func actionDidCancel(gesture _: UITapGestureRecognizer) {
        guard let presenterDelegate else {
            dismiss(animated: true, completion: nil)
            return
        }

        if presenterDelegate.presenterShouldHide(self) {
            dismiss(animated: true) {
                presenterDelegate.presenterDidHide(self)
            }
        }
    }

    // MARK: Interactive dismissal

    @objc func didPan(sender: Any?) {
        guard let panGestureRecognizer = sender as? UIPanGestureRecognizer else { return }
        guard let view = panGestureRecognizer.view else { return }

        handlePan(from: panGestureRecognizer, on: view)
    }

    private func handlePan(from panGestureRecognizer: UIPanGestureRecognizer, on view: UIView) {
        let translation = panGestureRecognizer.translation(in: view)
        let velocity = panGestureRecognizer.velocity(in: view)

        switch panGestureRecognizer.state {
        case .began,
             .changed:
            if sheetPresenterDelegate?.presenterCanDrag(self) == false {
                return
            }
            if let interactiveDismissal {
                let progress = min(
                    1.0,
                    max(0.0, (translation.y - initialTranslation.y) / max(1.0, view.bounds.size.height))
                )

                interactiveDismissal.update(progress)
            } else {
                if let presenterDelegate, !presenterDelegate.presenterShouldHide(self) {
                    break
                }

                interactiveDismissal = UIPercentDrivenInteractiveTransition()
                initialTranslation = translation
                presentedViewController.dismiss(animated: true)
            }
        case .cancelled,
             .ended:
            if let interactiveDismissal {
                let thresholdReached = interactiveDismissal.percentComplete >= configuration.dismissPercentThreshold
                let shouldDismiss = (thresholdReached && velocity.y >= 0) ||
                    (velocity.y >= configuration.dismissVelocityThreshold && translation.y >= configuration
                        .dismissMinimumOffset)
                stopPullToDismiss(finished: panGestureRecognizer.state != .cancelled && shouldDismiss)
            }
        default:
            break
        }
    }

    private func stopPullToDismiss(finished: Bool) {
        guard let interactiveDismissal else {
            return
        }

        self.interactiveDismissal = nil

        if finished {
            interactiveDismissal.completionSpeed = configuration.dismissFinishSpeedFactor
            interactiveDismissal.finish()

            presenterDelegate?.presenterDidHide(self)
        } else {
            interactiveDismissal.completionSpeed = configuration.dismissCancelSpeedFactor
            interactiveDismissal.cancel()
        }
    }
}

extension AutosizedSheetPresentationController: ModalPresenterProtocol {
    func hide(view _: ModalViewProtocol, animated: Bool, completion: (() -> Void)?) {
        guard interactiveDismissal == nil else {
            return
        }

        dismiss(animated: animated, completion: completion)
    }
}

// The delegate allows us to change preferredContentSize w/o subclassing
private final class NavigationDelegateProxy: NSObject, UINavigationControllerDelegate {
    weak var presentationController: AutosizedSheetPresentationController?
    weak var originalDelegate: UINavigationControllerDelegate?

    init(presentationController: AutosizedSheetPresentationController) {
        self.presentationController = presentationController
        super.init()
    }

    override func responds(to aSelector: Selector!) -> Bool {
        if super.responds(to: aSelector) { return true }
        return originalDelegate?.responds(to: aSelector) ?? false
    }

    override func forwardingTarget(for aSelector: Selector!) -> Any? {
        guard originalDelegate?.responds(to: aSelector) == true else {
            return nil
        }
        return originalDelegate
    }

    func navigationController(
        _ navigationController: UINavigationController,
        willShow viewController: UIViewController,
        animated: Bool
    ) {
        originalDelegate?.navigationController?(
            navigationController,
            willShow: viewController,
            animated: animated
        )
        presentationController?.handleNavigationTransition(animated: animated)
    }

    func navigationController(
        _ navigationController: UINavigationController,
        didShow viewController: UIViewController,
        animated: Bool
    ) {
        originalDelegate?.navigationController?(
            navigationController,
            didShow: viewController,
            animated: animated
        )
        if let presentationController {
            let targetSize = presentationController.resolvePreferredSize(for: viewController)
            if targetSize != .zero {
                navigationController.preferredContentSize = targetSize
            }
        }
        presentationController?.handleNavigationTransition(animated: false)
    }
}

extension AutosizedSheetPresentationController: UIGestureRecognizerDelegate {
    func gestureRecognizer(
        _: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith _: UIGestureRecognizer
    ) -> Bool {
        true
    }
}

private extension UIView {
    var firstDescendantScrollView: UIScrollView? {
        if let scrollView = self as? UIScrollView { return scrollView }
        return subviews.lazy.compactMap(\.firstDescendantScrollView).first
    }
}
