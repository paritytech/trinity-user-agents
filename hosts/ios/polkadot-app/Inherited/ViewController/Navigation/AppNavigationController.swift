import DesignSystem
import UIKit
import UIKitExt

protocol HiddableBarWhenPushed: AnyObject {}

protocol NavigationBarHiding: AnyObject {
    var hidesNavigationBar: Bool { get }
}

protocol NavigationDependable: AnyObject {
    var navigationControlling: NavigationControlling? { get set }
}

protocol NavigationControlling: AnyObject {
    var isNavigationBarHidden: Bool { get }

    func setNavigationBarHidden(_ hidden: Bool, animated: Bool)
}

/// A screen that animates its own push and pop. Nil keeps the default transition.
protocol NavigationTransitionAnimating: AnyObject {
    func navigationAnimator(for operation: UINavigationController.Operation) -> UIViewControllerAnimatedTransitioning?

    /// Whether a swipe from the left edge may start this screen's pop now. The swipe scrubs the pop when the
    /// screen's animator is a `BackSwipeScrubbing`.
    var allowsBackSwipe: Bool { get }
}

extension NavigationTransitionAnimating {
    var allowsBackSwipe: Bool {
        false
    }
}

protocol AppNavigationControllerTransitionObserving: AnyObject {
    func appNavigationController(
        _ navigationController: AppNavigationController,
        willShow viewController: UIViewController,
        animated: Bool
    )
    func appNavigationController(
        _ navigationController: AppNavigationController,
        didShow viewController: UIViewController,
        animated: Bool
    )
}

class AppNavigationController: UINavigationController {
    weak var transitionObserver: AppNavigationControllerTransitionObserving?

    private lazy var backSwipe = UIScreenEdgePanGestureRecognizer(target: self, action: #selector(followBackSwipe))

    /// The pop the back swipe is scrubbing, from when it started until the finger lets go.
    private var scrubbedPop: BackSwipeScrubbing?

    var closeButtonImage: UIImage = .buttonClose

    var barSettings: NavigationBarSettings = .defaultSettings {
        didSet {
            applyBarStyle()
        }
    }

    var scrollEdgeBarSettings: NavigationBarSettings? {
        didSet {
            applyBarStyle()
        }
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        setup()

        interactivePopGestureRecognizer?.delegate = nil
    }

    override var childForStatusBarStyle: UIViewController? {
        topViewController
    }

    override init(
        rootViewController: UIViewController
    ) {
        super.init(rootViewController: rootViewController)
        setupBackButtonItem(for: rootViewController)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)

        insertCloseButtonToRootIfNeeded()
    }

    override func setViewControllers(_ viewControllers: [UIViewController], animated: Bool) {
        viewControllers.forEach { setupBackButtonItem(for: $0) }
        super.setViewControllers(viewControllers, animated: animated)
    }

    func pushViewControllerWithCloseButton(_ viewController: UIViewController, animated: Bool) {
        if barSettings.shouldSetCloseButton {
            let closeItem = UIBarButtonItem(
                image: closeButtonImage.withRenderingMode(.alwaysTemplate),
                style: .plain,
                target: self,
                action: #selector(actionClose)
            )
            viewController.navigationItem.leftBarButtonItem = closeItem
        }
        pushViewController(viewController, animated: animated)
    }

    func applyBarStyle() {
        let tint = resolvedTintColor()
        navigationBar.tintColor = tint
        navigationBar.standardAppearance = themedAppearance(
            from: barSettings,
            tint: tint
        )
        navigationBar.scrollEdgeAppearance = themedAppearance(
            from: scrollEdgeBarSettings ?? barSettings,
            tint: tint
        )
    }
}

// MARK: - Private

private extension AppNavigationController {
    func setup() {
        delegate = self
        backSwipe.edges = .left
        backSwipe.delegate = self
        view.addGestureRecognizer(backSwipe)

        view.backgroundColor = .bgSurfaceMain

        applyBarStyle()

        registerForTraitChanges([DSThemeTrait.self]) { (controller: AppNavigationController, _) in
            controller.applyBarStyle()
        }
    }

    func themedAppearance(from settings: NavigationBarSettings, tint: UIColor?) -> UINavigationBarAppearance {
        let appearance = settings.barAppearance

        guard
            let tint,
            let backImage = settings.style.backImage
        else { return appearance }

        let tinted = backImage.withTintColor(tint, renderingMode: .alwaysOriginal)
        appearance.setBackIndicatorImage(tinted, transitionMaskImage: tinted)

        return appearance
    }

    func resolvedTintColor() -> UIColor? {
        guard barSettings.style.tintColor != nil else { return nil }

        return .fgPrimary.resolvedColor(with: traitCollection)
    }

    func updateNavigationBarState(in viewController: UIViewController) {
        let isHidden = viewController is HiddableBarWhenPushed
            || (viewController as? NavigationBarHiding)?.hidesNavigationBar == true
        setNavigationBarHidden(isHidden, animated: true)

        guard
            let navigationDependable = viewController as? NavigationDependable
        else {
            return
        }
        navigationDependable.navigationControlling = self
    }

    func setupBackButtonItem(for viewController: UIViewController) {
        let mode = UINavigationItem.BackButtonDisplayMode.minimal

        guard viewController.navigationItem.backButtonDisplayMode != mode else { return }

        viewController.navigationItem.backButtonDisplayMode = mode
    }

    func insertCloseButtonToRootIfNeeded() {
        guard
            barSettings.shouldSetCloseButton,
            presentingViewController != nil,
            let rootViewController = viewControllers.first,
            rootViewController.navigationItem.leftBarButtonItem == nil
        else {
            return
        }
        let closeItem = UIBarButtonItem(
            image: closeButtonImage,
            style: .plain,
            target: self,
            action: #selector(actionClose)
        )
        rootViewController.navigationItem.leftBarButtonItem = closeItem
    }

    @objc func actionClose() {
        presentingViewController?.dismiss(animated: true, completion: nil)
    }

    var isBackSwiping: Bool {
        backSwipe.state == .began || backSwipe.state == .changed
    }

    @objc func followBackSwipe(_ gesture: UIScreenEdgePanGestureRecognizer) {
        let width = view.bounds.width
        let travel = gesture.translation(in: view).x
        let progress = BackSwipe.progress(travel: travel, width: width)
        switch gesture.state {
        case .began:
            popViewController(animated: true)
        case .changed:
            scrubbedPop?.update(progress: progress)
        case .ended:
            let velocity = gesture.velocity(in: view).x
            let commits = BackSwipe.commits(travel: travel, velocity: velocity, width: width)
            releaseBackSwipe(progress: progress, commits: commits)
        case .cancelled,
             .failed:
            releaseBackSwipe(progress: progress, commits: false)
        default:
            break
        }
    }

    func releaseBackSwipe(progress: CGFloat, commits: Bool) {
        scrubbedPop?.release(progress: progress, commits: commits)
        scrubbedPop = nil
    }
}

// MARK: - UINavigationControllerDelegate

extension AppNavigationController: UINavigationControllerDelegate {
    func navigationController(
        _: UINavigationController,
        willShow viewController: UIViewController,
        animated: Bool
    ) {
        updateNavigationBarState(in: viewController)
        setupBackButtonItem(for: viewController)
        transitionObserver?.appNavigationController(
            self,
            willShow: viewController,
            animated: animated
        )

        guard viewControllers.count == 1 else {
            return
        }
        insertCloseButtonToRootIfNeeded()
    }

    func navigationController(
        _: UINavigationController,
        didShow viewController: UIViewController,
        animated: Bool
    ) {
        transitionObserver?.appNavigationController(
            self,
            didShow: viewController,
            animated: animated
        )
    }

    func navigationController(
        _: UINavigationController,
        animationControllerFor operation: UINavigationController.Operation,
        from fromViewController: UIViewController,
        to toViewController: UIViewController
    ) -> UIViewControllerAnimatedTransitioning? {
        let moving = operation == .push ? toViewController : fromViewController
        return (moving as? NavigationTransitionAnimating)?.navigationAnimator(for: operation)
    }

    func navigationController(
        _: UINavigationController,
        interactionControllerFor animationController: UIViewControllerAnimatedTransitioning
    ) -> UIViewControllerInteractiveTransitioning? {
        guard isBackSwiping else { return nil }

        scrubbedPop = animationController as? BackSwipeScrubbing
        return scrubbedPop
    }
}

// MARK: - UIGestureRecognizerDelegate

/// The stack's own back swipes have no delegate, so this serves only the swipe that scrubs a screen's own pop.
/// That swipe begins on a screen that allows it, and other pans wait for it.
extension AppNavigationController: UIGestureRecognizerDelegate {
    func gestureRecognizerShouldBegin(_: UIGestureRecognizer) -> Bool {
        guard viewControllers.count > 1, transitionCoordinator == nil else { return false }

        return (topViewController as? NavigationTransitionAnimating)?.allowsBackSwipe == true
    }

    func gestureRecognizer(
        _: UIGestureRecognizer,
        shouldBeRequiredToFailBy other: UIGestureRecognizer
    ) -> Bool {
        other is UIPanGestureRecognizer
    }
}

// MARK: - NavigationControlling

extension AppNavigationController: NavigationControlling {}

private extension NavigationBarSettings {
    var barAppearance: UINavigationBarAppearance {
        let appearance = UINavigationBarAppearance()

        if #available(iOS 26.0, *) {
            appearance.configureWithTransparentBackground()
        } else if let backgroundColor = style.backgroundColor {
            appearance.configureWithOpaqueBackground()
            appearance.backgroundColor = backgroundColor
            appearance.backgroundEffect = style.backgroundEffect
            appearance.shadowImage = style.shadow
            appearance.shadowColor = style.shadowColor
        } else {
            appearance.configureWithTransparentBackground()
        }

        let back = style.backImage
        appearance.setBackIndicatorImage(back, transitionMaskImage: back)

        if let titleAttributes = style.titleAttributes {
            appearance.titleTextAttributes = titleAttributes
        }

        if let titleAttributes = style.largeTitleAttributes {
            appearance.largeTitleTextAttributes = titleAttributes
        }

        return appearance
    }
}

extension AppNavigationController: ControllerBackedProtocol {}
