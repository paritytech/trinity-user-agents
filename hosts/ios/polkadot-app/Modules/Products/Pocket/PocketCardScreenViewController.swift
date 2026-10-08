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
    private let surface: PocketCardSurface
    private let face: UIHostingController<PocketOpenedCardView>

    private let scrollView = UIScrollView()
    private var productHeight: Constraint?

    private var requestedFaceShown: Bool
    private var openingFaceApplied = false
    private var userOwnsFace = false

    init(
        card: PocketCardViewModel,
        product: SPAViewProtocol,
        surface: PocketCardSurface,
        faceShown: Bool = true
    ) {
        self.card = card
        self.product = product
        self.surface = surface
        requestedFaceShown = faceShown
        let face = UIHostingController(rootView: PocketOpenedCardView(card: card))
        face.view.backgroundColor = .clear
        self.face = face

        super.init(nibName: nil, bundle: nil)

        surface.screen = self
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
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()

        guard !openingFaceApplied, scrollView.bounds.height > 0 else { return }

        openingFaceApplied = true
        moveFace(shown: requestedFaceShown, animated: false)
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)

        guard isBeingDismissed else { return }

        handBackProduct()
    }

    /// Gives the product back unattached. It outlives the screen that opened
    /// it, so a card opened again must find it free to show.
    func handBackProduct() {
        for child in children {
            child.willMove(toParent: nil)
            child.view.removeFromSuperview()
            child.removeFromParent()
        }

        if surface.screen === self {
            surface.screen = nil
        }
    }

    /// Shows or hides the face at the page's request, unless the user is
    /// moving it. A request before the first layout is kept for that layout.
    func setFaceShown(_ shown: Bool, animated: Bool) -> ExpandedCardFaceOutcome {
        guard !userOwnsFace else { return .userMoving }

        requestedFaceShown = shown

        if openingFaceApplied {
            moveFace(shown: shown, animated: animated)
        }

        return .applied
    }
}

// MARK: - UIScrollViewDelegate

extension PocketCardScreenViewController: UIScrollViewDelegate {
    func scrollViewWillBeginDragging(_: UIScrollView) {
        userOwnsFace = true
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
        guard !userOwnsFace else { return }

        fitProduct()
    }
}

// MARK: - Private

private extension PocketCardScreenViewController {
    func setupScrollView() {
        view.addSubview(scrollView)
        scrollView.snp.makeConstraints { make in
            make.top.equalTo(view.safeAreaLayoutGuide.snp.top)
            make.leading.trailing.bottom.equalToSuperview()
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

    func embed(_ child: UIViewController) {
        addChild(child)
        scrollView.addSubview(child.view)
        child.didMove(toParent: self)
    }

    /// An animated move is fitted when its animation ends. A move made at
    /// once, or to where the face already is, gets no such callback, so it is
    /// fitted here; nothing moves in between, so the page need not grow.
    func moveFace(shown: Bool, animated: Bool) {
        let target = CGPoint(x: 0, y: shown ? 0 : PocketOpenedCardView.height)

        guard animated, scrollView.contentOffset != target else {
            scrollView.contentOffset = target
            fitProduct()
            return
        }

        growProduct()
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
    static let horizontalInset: CGFloat = 24
    static let verticalInset: CGFloat = 16

    static var height: CGFloat { PocketCardSize.height + verticalInset * 2 }

    let card: PocketCardViewModel

    var body: some View {
        PocketCollectionCardView(card: card)
            .padding(.horizontal, Self.horizontalInset)
            .padding(.vertical, Self.verticalInset)
    }
}
