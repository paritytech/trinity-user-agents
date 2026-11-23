import FoundationExt
import Products
import SwiftUI
import enum TrUAPIHost.ExpandedCardFaceOutcome
import enum TrUAPIHost.ExpandedCardTurnOutcome
import struct TrUAPIHost.ProductId
import UIKit
import UIKitExt

final class ExpandedPocketCardViewController: UIViewController, ViewHolder {
    typealias RootViewType = ExpandedPocketCardViewLayout

    let presenter: ExpandedPocketCardPresenterProtocol
    let productId: ProductId
    weak var origin: PocketOrigin?

    private let card: PocketCardViewModel
    private let widgetView: WidgetView
    private let engine: SPAJSEngine
    private let face: UIHostingController<PocketOpenedCardView>
    private let turn: PocketCardTurn
    /// Holds the page so the face can join the page's safe area through `additionalSafeAreaInsets`
    /// without moving the face, which follows this screen's own safe area.
    private let pageController = UIViewController()

    private lazy var pullDown = PocketPullDown(screen: self) { [weak self] in self?.close() }
    private lazy var turnPan = UIPanGestureRecognizer(target: self, action: #selector(followTurn))
    private var faceSetByPage = false
    private var isClosing = false
    private var isInFlight = false

    init(
        presenter: ExpandedPocketCardPresenterProtocol,
        card: PocketCardViewModel,
        productId: ProductId,
        widgetView: WidgetView,
        engine: SPAJSEngine
    ) {
        self.presenter = presenter
        self.card = card
        self.productId = productId
        self.widgetView = widgetView
        self.engine = engine

        let turn = PocketCardTurn()
        self.turn = turn
        let face = UIHostingController(rootView: PocketOpenedCardView(
            card: card,
            turn: turn,
            onBackChanged: { turn.backChanged(available: $0) },
            onTurnOver: { turn.turnOver() }
        ))
        face.view.backgroundColor = .clear
        face.safeAreaRegions = []
        self.face = face

        super.init(nibName: nil, bundle: nil)

        pageController.view = widgetView
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = ExpandedPocketCardViewLayout(widgetView: widgetView)
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        title = card.title
        setupFace()
        setupPage()
        pullDown.install()
        setContentScrollView(widgetView.webView.scrollView, for: .top)
    }

    override func viewIsAppearing(_ animated: Bool) {
        super.viewIsAppearing(animated)

        guard isEnteringScreen else { return }

        presenter.didBecomeVisible()
    }

    override func viewSafeAreaInsetsDidChange() {
        super.viewSafeAreaInsetsDidChange()

        widgetView.scrollIndicatorInsets = view.safeAreaInsets
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        navigationController?.setSystemBackSwipesEnabled(false)
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)

        navigationController?.setSystemBackSwipesEnabled(true)
    }

    override func willMove(toParent parent: UIViewController?) {
        super.willMove(toParent: parent)

        // The list shows only fronts, and the screen is kept to open again on one.
        guard parent != nil else {
            turn.turnHome()
            return
        }

        origin?.setOpenItem(.card(card.key))
    }

    override func didMove(toParent parent: UIViewController?) {
        super.didMove(toParent: parent)

        guard parent == nil else { return }

        showCardInPocket()
    }
}

extension ExpandedPocketCardViewController: ExpandedPocketCardViewProtocol {
    /// From its push until its pop starts, even while another tab is selected or something it
    /// presents covers it.
    var isOnDisplay: Bool {
        navigationController?.viewControllers.contains(self) == true
    }

    func navigate(to url: URL) {
        widgetView.webView.load(URLRequest(url: url))
    }

    func navigate(to page: ProductPage) {
        guard let currentURL = widgetView.webView.url else { return }

        let target = page.applied(to: currentURL)
        guard target != currentURL else { return }

        navigate(to: target)
    }

    func reload() {
        widgetView.webView.reload()
    }

    func showLoading() {
        widgetView.showLoading()
    }

    func hideLoading() {
        widgetView.hideLoading()
    }

    func showLoadFailure(_ content: ErrorContent) {
        widgetView.showFailure(content)
    }

    func updateLoadPhase(_ phase: ProductLoadPhase) {
        widgetView.updateLoadPhase(phase)
    }

    func setFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome {
        ExpandedCardFaceOutcome(answer(.face(shown: shown)))
    }

    func setBackShown(_ shown: Bool) -> ExpandedCardTurnOutcome {
        answer(.back(shown: shown))
    }

    /// The card's published starting face, unless the page has already set it.
    func applyOpeningFace(shown: Bool) {
        guard isOnDisplay, !faceSetByPage else { return }

        setFaceCovered(!shown)
    }
}

extension ExpandedPocketCardViewController: ProductPageShowing {}

/// The face takes touches only while uncovered, so a sideways drag turns the card only then, and a vertical one is
/// left to the pull-down.
extension ExpandedPocketCardViewController: UIGestureRecognizerDelegate {
    func gestureRecognizerShouldBegin(_: UIGestureRecognizer) -> Bool {
        turn.canTurn && transitionCoordinator == nil && turnPan.isSideways(in: view)
    }

    func gestureRecognizer(_: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith _: UIGestureRecognizer) -> Bool {
        true
    }
}

extension ExpandedPocketCardViewController: PocketFlightScreen {
    var pocketItem: PocketItem {
        .card(card.key)
    }

    /// Without the margins the screen draws around the card.
    func itemFrame(in view: UIView) -> CGRect {
        let card = face.view.bounds.insetBy(
            dx: PocketCardSize.sideMargin,
            dy: PocketOpenedCardView.verticalInset
        )
        return face.view.convert(card, to: view)
    }

    /// The card flies over the page and lets the Pocket show behind it.
    func beginFlight() {
        view.backgroundColor = .clear
        rootView.bringFaceToFront()
    }

    func moveItem(by offset: CGPoint) {
        rootView.stageView.transform = CGAffineTransform(translationX: offset.x, y: offset.y)
    }

    /// The Pocket draws the card while it flies.
    func setItemHidden(_ hidden: Bool) {
        rootView.faceSlot.alpha = hidden ? 0 : 1
    }

    func setPageAlpha(_ alpha: CGFloat) {
        widgetView.alpha = alpha
    }

    func endFlight() {
        isInFlight = false
        view.backgroundColor = .bgSurfaceMain
        setPageAlpha(1)
        moveItem(by: .zero)
        setItemHidden(false)
        rootView.sendFaceBehindPage()
    }
}

extension ExpandedPocketCardViewController: NavigationTransitionAnimating {
    func navigationAnimator(for operation: UINavigationController.Operation) -> UIViewControllerAnimatedTransitioning? {
        isInFlight = true
        return PocketFlight(screen: self, origin: origin, opening: operation == .push)
    }
}

private extension ExpandedPocketCardViewController {
    func setupFace() {
        addChild(face)
        rootView.embedFace(face.view)
        face.didMove(toParent: self)

        turnPan.delegate = self
        face.view.addGestureRecognizer(turnPan)
    }

    /// Answers the page as the change starts, without waiting for it to finish.
    func answer(_ request: ExpandedCardPageRequest) -> ExpandedCardTurnOutcome {
        let outcome = request.outcome(
            hasBack: turn.hasBack,
            isUserMoving: pullDown.isPulling || turn.isUserMoving,
            isPresented: isOnDisplay && !isClosing
        )
        guard outcome == .applied else { return outcome }

        switch request {
        case let .face(shown):
            faceSetByPage = true
            setFaceCovered(!shown)
        case let .back(shown):
            turn.turn(to: shown ? .back : .front)
        }
        return outcome
    }

    @objc func followTurn(_ pan: UIPanGestureRecognizer) {
        let width = face.view.bounds.width - 2 * PocketCardSize.sideMargin
        switch pan.state {
        case .began:
            turn.beginDrag()
        case .changed:
            turn.drag(by: pan.translation(in: view).x, width: width)
            pan.setTranslation(.zero, in: view)
        case .ended,
             .cancelled,
             .failed:
            turn.release(velocity: pan.velocity(in: view).x, width: width)
        default:
            break
        }
    }

    func setupPage() {
        widgetView.onScroll = { [weak self] offset in self?.rootView.followPage(offset: offset) }
        engine.onNavigationIntercepted = { [weak self] url in self?.presenter.didInterceptNavigation(to: url) }
        engine.onDeeplinkRequested = { [weak self] url in self?.presenter.didRequestDeeplink(url) }

        addChild(pageController)
        pageController.didMove(toParent: self)
        pageController.additionalSafeAreaInsets.top = PocketOpenedCardView.height

        presenter.setup(engine: engine)
    }

    func setFaceCovered(_ covered: Bool) {
        rootView.isFaceCovered = covered
        pullDown.isEnabled = !covered
    }

    func close() {
        guard !isInFlight, !isClosing else { return }

        isClosing = true
        navigationController?.popViewController(animated: true)
    }

    /// Leaves the Pocket alone once another card's screen has taken it over.
    func showCardInPocket() {
        guard let origin, origin.openItem == .card(card.key) else { return }

        origin.moveOpenItem(by: .zero, clearingPocket: false, animated: false)
        origin.setOpenItem(nil)
    }
}

private extension UIPanGestureRecognizer {
    /// Only a sideways pan turns the card: a vertical one scrolls the page or pulls the card down to close it.
    func isSideways(in view: UIView) -> Bool {
        let panVelocity = velocity(in: view)
        return abs(panVelocity.x) > abs(panVelocity.y)
    }
}
