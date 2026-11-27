import DesignSystem
import PolkadotUI
import Products
import SnapKit
import UIKit
import WebKit

/// The Collectibles screen pushed over the Wallet, with the stash product filling it.
///
/// The screen opens like a book. Its cover swings open while the product loads, and the page grows to fill the screen
/// once the load has finished. A load that does not finish in time fails the page instead.
final class CollectiblesViewController: UIViewController {
    private let product: SPAViewProtocol
    private weak var origin: PocketOrigin?

    private let pageView = UIView()
    private let skeleton = CollectiblesSkeletonView(frame: .zero)
    private let book = CollectiblesBookView(frame: .zero)

    private var opening = CollectiblesOpening()
    private var hasStartedOpening = false
    private var pageExpanded = false
    private var isBookAnimating = false
    private var pendingClose: (() -> Void)?
    private var loadObservation: NSKeyValueObservation?
    private var loadTimeout: Task<Void, Never>?
    private var pageTopBelowBars: Constraint?
    private var pageTopAtScreenTop: Constraint?
    /// The page's shrink while a back swipe scrubs it. Nil when the swipe has only a resting page to fade.
    private var pageScrub: UIViewPropertyAnimator?
    /// Whether a back swipe has already drawn the expanded page where it rests, so the close does not shrink it.
    private var isPageShrunkBySwipe = false

    init(product: SPAViewProtocol, origin: PocketOrigin? = nil) {
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

    override func willMove(toParent parent: UIViewController?) {
        super.willMove(toParent: parent)

        guard parent != nil else { return }

        origin?.setOpenItem(.collectibles)
    }

    override func didMove(toParent parent: UIViewController?) {
        super.didMove(toParent: parent)

        guard parent == nil else { return }

        stopWatchingLoad()
        if let origin, origin.openItem == .collectibles {
            origin.moveOpenItem(by: .zero, clearingPocket: false, animated: false)
            origin.setOpenItem(nil)
        }
    }
}

extension CollectiblesViewController: PocketItemScreen {
    var pocketItem: PocketItem {
        .collectibles
    }

    var statusStripTitle: String {
        String(localized: .Products.collectibles)
    }
}

// MARK: - NavigationBarHiding

extension CollectiblesViewController: NavigationBarHiding {
    var hidesNavigationBar: Bool {
        true
    }
}

// MARK: - NavigationTransitionAnimating

extension CollectiblesViewController: NavigationTransitionAnimating {
    func navigationAnimator(for operation: UINavigationController.Operation) -> UIViewControllerAnimatedTransitioning? {
        switch operation {
        case .push:
            CollectiblesBookFlight(screen: self, origin: origin)
        case .pop:
            CollectiblesBookReturn(screen: self, origin: origin)
        default:
            nil
        }
    }

    var allowsBackSwipe: Bool {
        hasStartedOpening && !isBookAnimating && pendingClose == nil
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

    /// Winds the opening back to the book: the page shrinks and the cover closes if opened, then the book lifts.
    func close(then completion: @escaping () -> Void) {
        stopWatchingLoad()

        guard !isBookAnimating else {
            pendingClose = completion
            return
        }
        closeFromCurrentState(then: completion)
    }

    /// Readies the first stage of the close for a back swipe to scrub: an expanded page shrinks to its resting
    /// inset with the finger, and a resting page fades.
    func beginScrub() {
        guard pageExpanded else { return }

        let shrink = UIViewPropertyAnimator(
            duration: CollectiblesBookTiming.pageExpandDuration,
            curve: .easeIn
        ) { [self] in
            pageView.transform = shrinkTransform()
            pageView.layer.cornerRadius = CollectiblesBookTiming.pageCornerRadius / restingPageScale
        }
        pageScrub = shrink
        shrink.pauseAnimation()
    }

    /// Draws the first stage of the close at `progress`, from 0, the page as it was, to 1.
    func scrub(to progress: CGFloat) {
        if let pageScrub {
            pageScrub.fractionComplete = progress
        } else {
            pageView.alpha = 1 - progress
        }
    }

    /// Runs the first stage to its end, or back to where it started, then calls `completion`.
    func endScrub(finishing: Bool, then completion: @escaping () -> Void) {
        guard let shrink = pageScrub else {
            if finishing {
                completion()
            } else {
                restorePageAlpha(then: completion)
            }
            return
        }

        let remaining = finishing ? 1 - shrink.fractionComplete : shrink.fractionComplete
        if remaining > 0 {
            shrink.isReversed = !finishing
            shrink.addCompletion { [self] _ in
                finishPageScrub(finishing: finishing, then: completion)
            }
            shrink.continueAnimation(withTimingParameters: nil, durationFactor: remaining)
        } else {
            shrink.stopAnimation(false)
            shrink.finishAnimation(at: finishing ? .end : .start)
            finishPageScrub(finishing: finishing, then: completion)
        }
    }
}

// MARK: - Private

private extension CollectiblesViewController {
    func setupPage() {
        view.addSubview(pageView)
        pageView.snp.makeConstraints { make in
            pageTopBelowBars = make.top.equalTo(view.safeAreaLayoutGuide).constraint
            pageTopAtScreenTop = make.top.equalToSuperview().constraint
            make.leading.trailing.bottom.equalToSuperview()
        }
        pageTopAtScreenTop?.deactivate()
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
        isBookAnimating = true
        book.lift { [weak self] in
            guard let self, !runPendingClose() else { return }

            openCover()
        }
    }

    func openCover() {
        book.openCover { [weak self] in
            guard let self, !runPendingClose() else { return }

            isBookAnimating = false
            coverDidOpen()
        }

        let reveal = UIViewPropertyAnimator(duration: CollectiblesBookTiming.coverDuration, curve: .linear) { [self] in
            pageView.alpha = 1
        }
        reveal.startAnimation()
    }

    /// With Reduce Motion the book is not drawn, the page is shown at once, and only the skeleton fades away.
    func openWithoutMotion() {
        pageExpanded = true
        setPageReachesScreenTop(true)
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

    /// Runs the close asked for while a book animation was in flight, in place of the rest of the opening.
    func runPendingClose() -> Bool {
        guard let completion = pendingClose else { return false }

        pendingClose = nil
        isBookAnimating = false
        closeFromCurrentState(then: completion)
        return true
    }

    /// Winds the opening back from the book's current state. An open cover closes first, then the book lifts.
    func closeFromCurrentState(then completion: @escaping () -> Void) {
        let coverOpened = book.isHidden
        collapsePage { [self] in
            guard coverOpened else {
                book.lift(then: completion)
                return
            }

            let fade = UIViewPropertyAnimator(
                duration: CollectiblesBookTiming.coverDuration,
                curve: .linear
            ) { [self] in
                pageView.alpha = 0
            }
            fade.startAnimation()
            book.closeCover { [self] in
                book.lift(then: completion)
            }
        }
    }

    func apply(_ event: CollectiblesOpening.Event) {
        guard let step = opening.handle(event) else { return }

        switch step {
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
            setPageReachesScreenTop(true)
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

    /// An expanded page reaches the top of the screen, so the product draws under the bars. A resting page starts
    /// below them, where the book sits.
    func setPageReachesScreenTop(_ reachesTop: Bool) {
        if reachesTop {
            pageTopBelowBars?.deactivate()
            pageTopAtScreenTop?.activate()
        } else {
            pageTopAtScreenTop?.deactivate()
            pageTopBelowBars?.activate()
        }
        view.layoutIfNeeded()
    }

    /// The book sits at the top of the screen, inset as the Pocket's peek is, its art scaled to its width.
    func layoutBook() {
        let inset = CollectiblesBookTiming.pageInset
        let width = view.bounds.width - 2 * inset
        let imageSize = UIImage.collectiblesBook.size
        let size = CGSize(width: width, height: width * imageSize.height / imageSize.width)
        book.bounds = CGRect(origin: .zero, size: size)
        book.center = CGPoint(x: view.bounds.midX, y: view.safeAreaInsets.top + inset + size.height / 2)
    }

    /// Shrinks the page to the book's inset, scaling from its top centre, so the page sits under the book.
    func layoutRestingPage() {
        pageView.transform = restingPageTransform()
        pageView.layer.cornerRadius = CollectiblesBookTiming.pageCornerRadius / restingPageScale
    }

    /// The scale that fits an expanded page inside the book's inset on each side.
    var restingPageScale: CGFloat {
        let width = view.bounds.width
        return (width - 2 * CollectiblesBookTiming.pageInset) / width
    }

    func restingPageTransform() -> CGAffineTransform {
        let width = view.bounds.width
        return CGAffineTransform(translationX: -width / 2, y: 0)
            .scaledBy(x: restingPageScale, y: restingPageScale)
            .translatedBy(x: width / 2, y: CollectiblesBookTiming.pageInset)
    }

    /// The transform that draws an expanded page where it rests, without laying its content out again.
    func shrinkTransform() -> CGAffineTransform {
        let bounds = view.bounds
        let top = view.safeAreaInsets.top
        let restingFrame = CGRect(x: 0, y: top, width: bounds.width, height: bounds.height - top)
        let restingDrawn = restingFrame
            .offsetBy(dx: -restingFrame.midX, dy: -restingFrame.midY)
            .applying(restingPageTransform())
            .offsetBy(dx: restingFrame.midX, dy: restingFrame.midY)
        let scale = restingPageScale
        return CGAffineTransform(
            a: scale,
            b: 0,
            c: 0,
            d: scale,
            tx: restingDrawn.minX - (1 - scale) * bounds.width / 2,
            ty: restingDrawn.minY - (1 - scale) * bounds.height / 2
        )
    }

    /// Shrinks an expanded page back to its resting inset, unless a back swipe already drew it there, then calls
    /// `completion`.
    func collapsePage(then completion: @escaping () -> Void) {
        guard pageExpanded, !isPageShrunkBySwipe else {
            completion()
            return
        }

        pageExpanded = false
        let collapse = makeCollapseAnimator()
        collapse.addCompletion { _ in
            completion()
        }
        collapse.startAnimation()
    }

    /// The shrink of an expanded page to its resting inset, which the timed close runs.
    func makeCollapseAnimator() -> UIViewPropertyAnimator {
        UIViewPropertyAnimator(
            duration: CollectiblesBookTiming.pageExpandDuration,
            curve: .easeIn
        ) { [self] in
            setPageReachesScreenTop(false)
            layoutRestingPage()
        }
    }

    /// Brings a resting page back to full opacity after a back swipe that did not commit, then calls `completion`.
    func restorePageAlpha(then completion: @escaping () -> Void) {
        let restore = UIViewPropertyAnimator(
            duration: CollectiblesBookTiming.coverDuration,
            curve: .linear
        ) { [self] in
            pageView.alpha = 1
        }
        restore.addCompletion { _ in
            completion()
        }
        restore.startAnimation()
    }

    /// Clears the page's shrink once the close has ended. A finished close leaves the page shrunk for the close to
    /// skip, and a cancelled one lets it rest at full size.
    func finishPageScrub(finishing: Bool, then completion: @escaping () -> Void) {
        pageScrub = nil
        if finishing {
            isPageShrunkBySwipe = true
        } else {
            pageView.transform = .identity
            pageView.layer.cornerRadius = 0
        }
        completion()
    }

    func stopWatchingLoad() {
        loadObservation = nil
        loadTimeout?.cancel()
    }
}
