import DesignSystem
import SnapKit
import SwiftUI
import TrUAPIHost
import UIKit

/// A card opened: the card itself at the top of the screen, with its product
/// filling the space below.
///
/// The card keeps drawing up here, so it stays live for as long as it is open.
/// Card and product share one scroll, so the gesture that reads the product
/// also carries the card away and leaves the product the whole screen.
///
/// The product is sized to the screen the card leaves it, so a page built to
/// fit its viewport keeps its bottom edge in view. It is resized only when the
/// card comes to rest, since every resize makes the page lay itself out again.
final class PocketCardScreenViewController: UIViewController {
    private let card: PocketCardViewModel
    private let product: SPAViewProtocol
    private let face: UIHostingController<PocketOpenedCardView>
    private weak var origin: PocketCardOrigin?

    private let scrollView = UIScrollView()
    private let backSwipe = UIScreenEdgePanGestureRecognizer()
    private var productHeight: Constraint?

    private var pendingFaceShown: Bool? = true
    private var userOwnsFace = false
    private var faceMovedSinceOpening = false
    private var pageMoveTarget: CGPoint?
    private var isInFlight = false

    init(card: PocketCardViewModel, product: SPAViewProtocol, origin: PocketCardOrigin? = nil) {
        self.card = card
        self.product = product
        self.origin = origin
        let face = UIHostingController(rootView: PocketOpenedCardView(card: card))
        face.view.backgroundColor = .clear
        face.safeAreaRegions = []
        self.face = face

        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        view.backgroundColor = .bgSurfaceMain
        title = card.title

        setupScrollView()
        setupFace()
        setupProduct()
        setupBackSwipe()

        // The card flies over the page when it is opened and closed.
        scrollView.bringSubviewToFront(face.view)
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()

        applyPendingFace(animated: false)
    }

    // The stack's own swipes would pop the card beside its back swipe.
    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        setSystemBackSwipesEnabled(false)
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)

        setSystemBackSwipesEnabled(true)
    }

    override func willMove(toParent parent: UIViewController?) {
        super.willMove(toParent: parent)

        guard parent != nil else { return }

        origin?.setOpenCard(card.key)
    }

    override func didMove(toParent parent: UIViewController?) {
        super.didMove(toParent: parent)

        guard parent == nil else { return }

        handBackProduct()
        showCardInPocket()
    }

    /// Gives the product back unattached. It outlives the screen that opened
    /// it, so a card opened again must find it free to show.
    func handBackProduct() {
        for child in children {
            child.willMove(toParent: nil)
            child.view.removeFromSuperview()
            child.removeFromParent()
        }
    }

    func setFaceShown(_ shown: Bool, animated: Bool) -> ExpandedCardFaceOutcome {
        guard !userOwnsFace else { return .userMoving }

        faceMovedSinceOpening = true
        placeFace(shown: shown, animated: animated)

        return .applied
    }

    /// The face the card's product published for its opening, which no longer
    /// applies once the card is closed or the user or the page has moved the face.
    func applyOpeningFace(shown: Bool) {
        guard isOnDisplay, !faceMovedSinceOpening else { return }

        placeFace(shown: shown, animated: true)
    }

    /// From its push until its pop starts, even while another tab is selected
    /// or something it presents covers it.
    var isOnDisplay: Bool {
        navigationController?.viewControllers.contains(self) == true
    }
}

extension PocketCardScreenViewController: TabBarRootStandIn {}

// MARK: - NavigationTransitionAnimating

extension PocketCardScreenViewController: NavigationTransitionAnimating {
    func navigationAnimator(for operation: UINavigationController.Operation) -> UIViewControllerAnimatedTransitioning? {
        flight(opening: operation == .push)
    }

    private func flight(opening: Bool) -> PocketCardFlight? {
        guard let origin else { return nil }

        guard !UIAccessibility.isReduceMotionEnabled else {
            // Nothing carries the card back, so the Pocket is whole again before the screen slides off it.
            origin.moveOpenCard(by: .zero, clearingPocket: false, animated: false)
            return nil
        }

        // From here on, not from when the flight starts: a face moved in
        // between would fly in already folded away.
        isInFlight = true
        return PocketCardFlight(screen: self, origin: origin, key: card.key, opening: opening)
    }
}

// MARK: - Card flight

extension PocketCardScreenViewController {
    /// Without the margins the screen draws around the card.
    func cardFrame(in view: UIView) -> CGRect {
        let card = face.view.bounds.insetBy(
            dx: PocketCardSize.sideMargin,
            dy: PocketOpenedCardView.verticalInset
        )
        return face.view.convert(card, to: view)
    }

    /// Lets the Pocket show behind the card, and stops the face where it is.
    func beginFlight() {
        scrollView.setContentOffset(scrollView.contentOffset, animated: false)
        scrollView.clipsToBounds = false
        view.backgroundColor = .clear
    }

    /// Moves the card with its page hanging off it, so the page never covers
    /// the card the Pocket draws in its place.
    func moveCard(by offset: CGPoint) {
        face.view.transform = CGAffineTransform(translationX: offset.x, y: offset.y)
        product.controller.view.transform = face.view.transform
    }

    /// The Pocket draws the card while it flies.
    func setCardHidden(_ hidden: Bool) {
        face.view.alpha = hidden ? 0 : 1
    }

    func setPageAlpha(_ alpha: CGFloat) {
        product.controller.view.alpha = alpha
    }

    /// Restores all the flight changed, since the next open reuses the product.
    func endFlight() {
        isInFlight = false
        scrollView.clipsToBounds = true
        view.backgroundColor = .bgSurfaceMain
        setPageAlpha(1)
        moveCard(by: .zero)
        setCardHidden(false)
        applyPendingFace(animated: true)
    }

    /// Leaves the Pocket alone once another card's screen has taken it over.
    func showCardInPocket() {
        guard let origin, origin.openCard == card.key else { return }

        origin.moveOpenCard(by: .zero, clearingPocket: false, animated: false)
        origin.setOpenCard(nil)
    }
}

// MARK: - UIScrollViewDelegate

extension PocketCardScreenViewController: UIScrollViewDelegate {
    func scrollViewWillBeginDragging(_: UIScrollView) {
        userOwnsFace = true
        faceMovedSinceOpening = true
        pageMoveTarget = nil
        growProduct()
    }

    func scrollViewDidEndDragging(_: UIScrollView, willDecelerate decelerate: Bool) {
        guard !decelerate else { return }

        userOwnsFace = false
        fitProduct()
    }

    func scrollViewDidEndDecelerating(_: UIScrollView) {
        userOwnsFace = false
        fitProduct()
    }

    func scrollViewDidEndScrollingAnimation(_: UIScrollView) {
        pageMoveTarget = nil

        guard !userOwnsFace else { return }

        fitProduct()
    }
}

// MARK: - Private

private extension PocketCardScreenViewController {
    func setupScrollView() {
        view.addSubview(scrollView)
        scrollView.snp.makeConstraints { make in
            make.top.bottom.equalTo(view.safeAreaLayoutGuide)
            make.leading.trailing.equalToSuperview()
        }

        scrollView.delegate = self
        scrollView.contentInsetAdjustmentBehavior = .never
        scrollView.showsVerticalScrollIndicator = false

        // The product's page keeps its own scrolling for content taller than
        // the screen, but must not rubber-band a page that fits: that would
        // swallow the gesture meant to carry the card away.
        product.pageScrollView.bounces = false
    }

    /// The scroll is a screenful plus the face, whatever the product's own
    /// height, so the face can always be scrolled away.
    func setupFace() {
        embed(face)

        face.view.snp.makeConstraints { make in
            make.top.leading.trailing.equalTo(scrollView.contentLayoutGuide)
            make.width.equalTo(scrollView.frameLayoutGuide)
            make.height.equalTo(PocketOpenedCardView.height)
        }

        scrollView.contentLayoutGuide.snp.makeConstraints { make in
            make.height.equalTo(scrollView.frameLayoutGuide).offset(PocketOpenedCardView.height)
        }
    }

    func setupProduct() {
        embed(product.controller)

        product.controller.view.snp.makeConstraints { make in
            make.top.equalTo(face.view.snp.bottom)
            make.leading.trailing.equalTo(scrollView.contentLayoutGuide)
            make.width.equalTo(scrollView.frameLayoutGuide)
            productHeight = make.height.equalTo(scrollView.frameLayoutGuide)
                .offset(-PocketOpenedCardView.height).constraint
        }
    }

    /// Closes the card once the swipe ends, as Android's back gesture does,
    /// so the close plays in full rather than following the finger.
    func setupBackSwipe() {
        backSwipe.edges = .left
        backSwipe.addTarget(self, action: #selector(swipedBack))
        view.addGestureRecognizer(backSwipe)
        scrollView.panGestureRecognizer.require(toFail: backSwipe)
        product.pageScrollView.panGestureRecognizer.require(toFail: backSwipe)
    }

    @objc func swipedBack() {
        guard backSwipe.state == .ended, backSwipe.velocity(in: view).x >= 0 else { return }

        navigationController?.popViewController(animated: true)
    }

    func setSystemBackSwipesEnabled(_ enabled: Bool) {
        navigationController?.interactivePopGestureRecognizer?.isEnabled = enabled
        if #available(iOS 26, *) {
            navigationController?.interactiveContentPopGestureRecognizer?.isEnabled = enabled
        }
    }

    func embed(_ child: UIViewController) {
        addChild(child)
        scrollView.addSubview(child.view)
        child.didMove(toParent: self)
    }

    func placeFace(shown: Bool, animated: Bool) {
        if pendingFaceShown == nil {
            moveFace(shown: shown, animated: animated)
        } else {
            pendingFaceShown = shown
        }
    }

    func applyPendingFace(animated: Bool) {
        guard let pendingFaceShown, !isInFlight, scrollView.bounds.height > 0 else { return }

        self.pendingFaceShown = nil
        moveFace(shown: pendingFaceShown, animated: animated)
    }

    /// A move made at once, or to where the face already is, gets no
    /// end-of-animation callback, so it is fitted here. A move under way is
    /// judged by its target, since the offset only follows from its first frame.
    func moveFace(shown: Bool, animated: Bool) {
        let target = CGPoint(x: 0, y: shown ? 0 : PocketOpenedCardView.height)

        guard target != pageMoveTarget else { return }

        guard animated, pageMoveTarget != nil || scrollView.contentOffset != target else {
            pageMoveTarget = nil
            scrollView.contentOffset = target
            fitProduct()
            return
        }

        growProduct()
        pageMoveTarget = target
        scrollView.setContentOffset(target, animated: true)
    }

    /// Lays the product under the whole screen while the face moves, so no
    /// blank strip opens below it before it is fitted again.
    func growProduct() {
        productHeight?.update(offset: 0)
    }

    func fitProduct() {
        let faceHeight = PocketOpenedCardView.height
        let visibleFace = min(max(faceHeight - scrollView.contentOffset.y, 0), faceHeight)

        productHeight?.update(offset: -visibleFace)
    }
}

/// The card as an opened screen draws it: the frame the collection gives it,
/// with the same margins the collection leaves around it.
struct PocketOpenedCardView: View {
    static let verticalInset: CGFloat = 16

    static var height: CGFloat { PocketCardSize.height + verticalInset * 2 }

    let card: PocketCardViewModel

    var body: some View {
        PocketCollectionCardView(card: card)
            .padding(.horizontal, PocketCardSize.sideMargin)
            .padding(.vertical, Self.verticalInset)
    }
}
