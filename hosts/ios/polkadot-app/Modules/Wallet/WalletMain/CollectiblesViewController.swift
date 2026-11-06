import DesignSystem
import PolkadotUI
import Products
import SnapKit
import UIKit
import WebKit

/// The Collectibles screen pushed over the Wallet, with the stash product filling it.
///
/// The screen opens like a book. Its cover swings open while the product loads, and the page grows to fill the screen
/// once the load has finished and the frames have settled. A load that does not finish in time fails the page instead.
final class CollectiblesViewController: UIViewController {
    private let product: SPAViewProtocol
    private weak var origin: CollectiblesOrigin?

    private let pageView = UIView()
    private let skeleton = CollectiblesSkeletonView(frame: .zero)
    private let book = CollectiblesBookView(frame: .zero)

    private var opening = CollectiblesOpening()
    private var hasStartedOpening = false
    private var pageExpanded = false
    private var loadObservation: NSKeyValueObservation?
    private var loadTimeout: Task<Void, Never>?
    private let frameWatch = FrameIntervalWatch()
    private var steadyFrames = SteadyFrameRule()

    init(product: SPAViewProtocol, origin: CollectiblesOrigin? = nil) {
        self.product = product
        self.origin = origin

        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        view.backgroundColor = .bgSurfaceMain
        setupPage()
        view.addSubview(book)
        observePageLoad()
    }

    override func viewIsAppearing(_ animated: Bool) {
        super.viewIsAppearing(animated)

        guard !hasStartedOpening else { return }

        hasStartedOpening = true
        if UIAccessibility.isReduceMotionEnabled {
            openWithoutMotion()
        } else {
            openBook()
        }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()

        layoutBook()
        guard !pageExpanded else { return }

        layoutRestingPage()
    }

    override func didMove(toParent parent: UIViewController?) {
        super.didMove(toParent: parent)

        guard parent == nil else { return }

        loadObservation = nil
        loadTimeout?.cancel()
        frameWatch.cancel()
    }
}

extension CollectiblesViewController: TabBarRootStandIn {}

// MARK: - NavigationTransitionAnimating

extension CollectiblesViewController: NavigationTransitionAnimating {
    func navigationAnimator(for operation: UINavigationController.Operation) -> UIViewControllerAnimatedTransitioning? {
        guard operation == .push, let origin else { return nil }

        return CollectiblesBookFlight(screen: self, origin: origin)
    }
}

// MARK: - Book flight

extension CollectiblesViewController {
    /// The book's frame in `view`, where it rests.
    func bookFrame(in view: UIView) -> CGRect {
        book.convert(book.bounds, to: view)
    }

    /// Lets the Pocket show behind the book, and starts the book `offset` away from its place.
    func beginFlight(from offset: CGPoint) {
        view.backgroundColor = .clear
        moveBook(by: offset)
    }

    /// Moves the book `offset` away from its place. Zero puts it back.
    func moveBook(by offset: CGPoint) {
        book.setFlightOffset(offset)
    }
}

// MARK: - Private

private extension CollectiblesViewController {
    func setupPage() {
        view.addSubview(pageView)
        pageView.snp.makeConstraints { make in
            make.top.equalTo(view.safeAreaLayoutGuide)
            make.leading.trailing.bottom.equalToSuperview()
        }
        pageView.alpha = 0
        pageView.clipsToBounds = true
        pageView.layer.cornerCurve = .continuous

        addChild(product.controller)
        pageView.addSubview(product.controller.view)
        product.controller.view.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
        product.controller.didMove(toParent: self)

        pageView.addSubview(skeleton)
        skeleton.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }

    func observePageLoad() {
        loadObservation = product.pageWebView.observe(\.isLoading, options: [.initial]) { [weak self] webView, _ in
            MainActor.assumeIsolated {
                self?.apply(.pageLoading(webView.isLoading))
            }
        }
    }

    func openBook() {
        skeleton.startSweep()
        book.lift { [weak self] in
            self?.openCover()
        }
    }

    func openCover() {
        frameWatch.watch(
            cap: nil,
            until: { [weak self] interval in self?.steadyFrames.sample(interval) ?? false },
            then: {}
        )

        book.openCover { [weak self] in
            self?.coverDidOpen()
        }

        let reveal = UIViewPropertyAnimator(duration: CollectiblesBookTiming.coverDuration, curve: .linear) { [self] in
            pageView.alpha = 1
        }
        reveal.startAnimation()
    }

    /// With Reduce Motion the book is not drawn, the page is shown at once, and only the skeleton fades away.
    func openWithoutMotion() {
        pageExpanded = true
        book.isHidden = true
        pageView.transform = .identity
        pageView.layer.cornerRadius = 0
        pageView.alpha = 1
        skeleton.startSweep()
        coverDidOpen()
    }

    func coverDidOpen() {
        loadTimeout = Task { [weak self] in
            try? await Task.sleep(for: .seconds(CollectiblesBookTiming.loadTimeout))
            guard !Task.isCancelled else { return }

            self?.apply(.loadTimedOut)
        }
        apply(.coverOpened)
    }

    func apply(_ event: CollectiblesOpening.Event) {
        guard let step = opening.handle(event) else { return }

        switch step {
        case .awaitSteadyFrames:
            frameWatch.watch(
                cap: CollectiblesBookTiming.settleCap,
                until: { [weak self] interval in self?.steadyFrames.isSteady(after: interval) ?? false },
                then: { [weak self] in self?.apply(.framesSettled) }
            )
        case .expandPage:
            expandPage()
        case .showFailure:
            skeleton.showFailure(String(localized: .Products.productLoadDamagedTitle))
        }
    }

    func expandPage() {
        pageExpanded = true
        let expand = UIViewPropertyAnimator(
            duration: CollectiblesBookTiming.pageExpandDuration,
            curve: .easeOut
        ) { [self] in
            pageView.transform = .identity
            pageView.layer.cornerRadius = 0
            skeleton.alpha = 0
        }
        expand.addCompletion { [self] _ in
            skeleton.stopSweep()
            skeleton.removeFromSuperview()
            apply(.pageExpanded)
        }
        expand.startAnimation()
    }

    /// The book sits at the top of the screen, inset as the Pocket's peek is.
    func layoutBook() {
        let inset = CollectiblesBookTiming.pageInset
        let size = CGSize(width: view.bounds.width - 2 * inset, height: UIImage.collectiblesBook.size.height)
        book.bounds = CGRect(origin: .zero, size: size)
        book.center = CGPoint(x: view.bounds.midX, y: view.safeAreaInsets.top + inset + size.height / 2)
    }

    /// Shrinks the page to the book's inset, scaling from its top centre, so the page sits under the book.
    func layoutRestingPage() {
        let width = view.bounds.width
        let scale = (width - 2 * CollectiblesBookTiming.pageInset) / width
        pageView.transform = CGAffineTransform(translationX: -width / 2, y: 0)
            .scaledBy(x: scale, y: scale)
            .translatedBy(x: width / 2, y: CollectiblesBookTiming.pageInset)
        pageView.layer.cornerRadius = CollectiblesBookTiming.pageCornerRadius / scale
    }
}
