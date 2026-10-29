import DesignSystem
import SnapKit
import SwiftUI
import TrUAPIHost
import UIKit

/// A card opened: the card itself at the top of the screen, with its product
/// filling the space below.
///
/// The card keeps drawing up here, so it stays live for as long as it is open.
/// The user moves the face by dragging it or the navigation bar. A drag on
/// the product stays the product's, which may draw with it.
///
/// The product is sized to the screen the card leaves it, so a page built to
/// fit its viewport keeps its bottom edge in view. It is resized only when the
/// card comes to rest, since every resize makes the page lay itself out again.
final class PocketCardScreenViewController: UIViewController {
    private let card: PocketCardViewModel
    private let product: SPAViewProtocol
    private let face: UIHostingController<PocketOpenedCardView>
    private weak var origin: PocketCardOrigin?

    private let scrollView = FaceScrollView()
    private var productHeight: Constraint?

    private var pendingFaceShown: Bool? = true
    private var userOwnsFace = false
    private var faceMovedSinceOpening = false
    private var pageMoveTarget: CGPoint?
    private var barDragStart: CGFloat?
    private var isInFlight = false
    private weak var coveredScreen: UIViewController?

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
        setupBarDrag()

        // The card flies over the page when it is opened and closed.
        scrollView.bringSubviewToFront(face.view)
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()

        applyPendingFace(animated: false)
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)

        guard (navigationController ?? self).isBeingPresented else { return }

        origin?.setOpenCard(card.key)
        coverScreen(animated: animated)
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        guard (navigationController ?? self).isBeingPresented else { return }

        coveredScreen?.endAppearanceTransition()
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)

        guard (navigationController ?? self).isBeingDismissed else { return }

        coveredScreen?.beginAppearanceTransition(true, animated: animated)
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)

        guard (navigationController ?? self).isBeingDismissed else { return }

        handBackProduct()
        showCardInPocket()
        coveredScreen?.endAppearanceTransition()
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

    /// Follows a drag on the navigation bar by `distance` from where it started.
    func dragFace(by distance: CGFloat) {
        // A close can start mid-drag, and the card must fly from where it stopped.
        guard !isInFlight else { return }

        if barDragStart == nil {
            holdFace()
            scrollView.isScrollEnabled = false
        }
        let start = barDragStart ?? scrollView.contentOffset.y
        barDragStart = start

        let offset = min(max(start - distance, 0), PocketOpenedCardView.height)
        scrollView.setContentOffset(CGPoint(x: 0, y: offset), animated: false)
    }

    func releaseFace() {
        barDragStart = nil
        userOwnsFace = false
        scrollView.isScrollEnabled = true

        guard !isInFlight else { return }

        moveFace(shown: scrollView.contentOffset.y < PocketOpenedCardView.height / 2, animated: true)
    }

    /// From the start of its presentation until its dismissal starts, even
    /// while something it presents covers it. It is presented inside a
    /// navigation controller, so that is what is presented and dismissed.
    var isOnDisplay: Bool {
        let presented = navigationController ?? self
        return presented.presentingViewController != nil && !presented.isBeingDismissed
    }
}

// MARK: - UIViewControllerTransitioningDelegate

extension PocketCardScreenViewController: UIViewControllerTransitioningDelegate {
    func animationController(
        forPresented _: UIViewController,
        presenting _: UIViewController,
        source _: UIViewController
    ) -> UIViewControllerAnimatedTransitioning? {
        flight(opening: true)
    }

    func animationController(forDismissed _: UIViewController) -> UIViewControllerAnimatedTransitioning? {
        flight(opening: false)
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
        navigationController?.view.backgroundColor = .clear
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

    /// All the screen draws besides the card once its backgrounds are cleared.
    func setSurroundingsAlpha(_ alpha: CGFloat) {
        setBarAlpha(alpha)
        setPageAlpha(alpha)
    }

    func setBarAlpha(_ alpha: CGFloat) {
        navigationController?.navigationBar.alpha = alpha
    }

    func setPageAlpha(_ alpha: CGFloat) {
        product.controller.view.alpha = alpha
    }

    /// Restores all the flight changed, since the next open reuses the product.
    func endFlight() {
        isInFlight = false
        scrollView.clipsToBounds = true
        view.backgroundColor = .bgSurfaceMain
        navigationController?.view.backgroundColor = .bgSurfaceMain
        setSurroundingsAlpha(1)
        moveCard(by: .zero)
        setCardHidden(false)
        applyPendingFace(animated: true)
    }

    func showCardInPocket() {
        origin?.moveOpenCard(by: .zero, clearingPocket: false, animated: false)
        origin?.setOpenCard(nil)
    }
}

// MARK: - UIScrollViewDelegate

extension PocketCardScreenViewController: UIScrollViewDelegate {
    func scrollViewWillBeginDragging(_: UIScrollView) {
        holdFace()
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

// MARK: - UIGestureRecognizerDelegate

extension PocketCardScreenViewController: UIGestureRecognizerDelegate {
    func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        guard let pan = gestureRecognizer as? UIPanGestureRecognizer, !isInFlight, !userOwnsFace else { return false }

        let velocity = pan.velocity(in: view)
        return abs(velocity.y) > abs(velocity.x)
    }
}

// MARK: - Private

private extension PocketCardScreenViewController {
    /// A screen presented over the Pocket leaves it in the window, so UIKit does
    /// not tell it that it is covered. This screen does.
    func coverScreen(animated: Bool) {
        let presented = navigationController ?? self
        guard presented.modalPresentationStyle == .overFullScreen else { return }

        coveredScreen = presented.presentingViewController
        coveredScreen?.beginAppearanceTransition(false, animated: animated)
    }

    func setupScrollView() {
        view.addSubview(scrollView)
        scrollView.snp.makeConstraints { make in
            make.top.equalTo(view.safeAreaLayoutGuide.snp.top)
            make.leading.trailing.bottom.equalToSuperview()
        }

        scrollView.delegate = self
        scrollView.contentInsetAdjustmentBehavior = .never
        scrollView.showsVerticalScrollIndicator = false

        // The page does not rubber-band, as on Android.
        product.pageScrollView.bounces = false
    }

    func setupBarDrag() {
        let pan = UIPanGestureRecognizer(target: self, action: #selector(followBarDrag))
        pan.delegate = self
        navigationController?.navigationBar.addGestureRecognizer(pan)
    }

    @objc func followBarDrag(_ pan: UIPanGestureRecognizer) {
        switch pan.state {
        case .began,
             .changed:
            dragFace(by: pan.translation(in: view).y)
        case .ended,
             .cancelled:
            releaseFace()
        default:
            break
        }
    }

    /// The scroll is a screenful plus the face, whatever the product's own
    /// height, so the face can always be scrolled away.
    func setupFace() {
        embed(face)
        scrollView.face = face.view

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

    func embed(_ child: UIViewController) {
        addChild(child)
        scrollView.addSubview(child.view)
        child.didMove(toParent: self)
    }

    func holdFace() {
        userOwnsFace = true
        faceMovedSinceOpening = true
        pageMoveTarget = nil
        growProduct()
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

/// Scrolls only for drags that start on the face.
final class FaceScrollView: UIScrollView {
    weak var face: UIView?

    override func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        guard super.gestureRecognizerShouldBegin(gestureRecognizer) else { return false }
        guard gestureRecognizer === panGestureRecognizer else { return true }

        return movesFace(from: panGestureRecognizer.location(in: self))
    }

    /// Whether a drag starting at `point`, in this view's coordinates, is on the face.
    func movesFace(from point: CGPoint) -> Bool {
        face?.frame.contains(point) ?? false
    }
}
