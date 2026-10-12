import DesignSystem
import SnapKit
import SwiftUI
import UIKit

/// A card opened: the card itself at the top of the screen, with its product
/// filling the space below.
///
/// The card keeps drawing up here, so it stays live for as long as it is open.
/// Card and product share one scroll, so the gesture that reads the product
/// also carries the card away and leaves the product the whole screen.
final class PocketCardScreenViewController: UIViewController {
    private let card: PocketCardViewModel
    private let product: SPAViewProtocol
    private let face: UIHostingController<PocketOpenedCardView>

    private let scrollView = UIScrollView()

    init(card: PocketCardViewModel, product: SPAViewProtocol) {
        self.card = card
        self.product = product
        let face = UIHostingController(rootView: PocketOpenedCardView(card: card))
        face.view.backgroundColor = .clear
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
        stack(face, height: PocketOpenedCardView.height, below: nil)
        stack(product.controller, height: nil, below: face.view)
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

        scrollView.contentInsetAdjustmentBehavior = .never
        scrollView.showsVerticalScrollIndicator = false

        // The product's page keeps its own scrolling for content taller than
        // the screen, but must not rubber-band a page that fits: that would
        // swallow the gesture meant to carry the card away.
        product.pageScrollView.bounces = false
    }

    /// Stacks one child under the last, each as wide as the screen. A child
    /// with no height of its own is given the screen's, so the product is
    /// exactly a screenful and the card is what there is to scroll past.
    func stack(_ child: UIViewController, height: CGFloat?, below previous: UIView?) {
        addChild(child)
        scrollView.addSubview(child.view)
        child.didMove(toParent: self)

        child.view.snp.makeConstraints { make in
            if let previous {
                make.top.equalTo(previous.snp.bottom)
                make.bottom.equalTo(scrollView.contentLayoutGuide)
            } else {
                make.top.equalTo(scrollView.contentLayoutGuide)
            }

            make.leading.trailing.equalTo(scrollView.contentLayoutGuide)
            make.width.equalTo(scrollView.frameLayoutGuide)

            if let height {
                make.height.equalTo(height)
            } else {
                make.height.equalTo(scrollView.frameLayoutGuide)
            }
        }
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
