import DesignSystem
import Observation
import PolkadotUI
import SnapKit
import SwiftUI
import UIKit
import UIKitExt

/// The Balance card opened onto a screen of its own, titled Purse, pushed on the Wallet stack above the Pocket.
///
/// The scene stays with the Wallet, so sending and adding money push onto the Wallet stack above this screen. The
/// screen draws the card's opened face. The Pocket draws the card while it flies between its stack and this screen.
final class AssetDetailsScreenViewController: UIViewController {
    private let viewModel: AssetDetailsViewModel
    private weak var origin: PocketOrigin?
    private let state = AssetDetailsScreenState()
    private lazy var hosting = UIHostingController(
        rootView: AssetDetailsScreenRoot(viewModel: viewModel, state: state)
    )
    private lazy var pullDown = PocketPullDown(screen: self) { [weak self] in self?.close() }
    private let titleLabel: PolkadotUI.Label = .create { view in
        view.typography = .headlineSmall
        view.textColor = .fgPrimary
    }

    private var isInFlight = false

    init(viewModel: AssetDetailsViewModel, origin: PocketOrigin?) {
        self.viewModel = viewModel
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
        setupTitle()
        setupHosting()
        pullDown.install()
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(reduceTransparencyDidChange),
            name: UIAccessibility.reduceTransparencyStatusDidChangeNotification,
            object: nil
        )
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)

        applyBarBackground()
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

        guard parent != nil else { return }

        origin?.setOpenItem(.balance)
    }

    override func didMove(toParent parent: UIViewController?) {
        super.didMove(toParent: parent)

        guard parent == nil, let origin, origin.openItem == .balance else { return }

        origin.moveOpenItem(by: .zero, clearingPocket: false, animated: false)
        origin.setOpenItem(nil)
    }
}

extension AssetDetailsScreenViewController: TabBarRootStandIn {}

extension AssetDetailsScreenViewController: PocketFlightScreen {
    var pocketItem: PocketItem {
        .balance
    }

    /// The card's frame on screen, wherever scrolling and a pull have moved it. The flight uses its top-left corner.
    func itemFrame(in view: UIView) -> CGRect {
        let card = state.cardFrame.offsetBy(dx: hosting.view.transform.tx, dy: hosting.view.transform.ty)
        return view.convert(card, from: nil)
    }

    /// Lets the Pocket show behind the card.
    func beginFlight() {
        view.backgroundColor = .clear
    }

    /// Moves the card with its page, so the page never covers the card the Pocket draws in its place.
    func moveItem(by offset: CGPoint) {
        hosting.view.transform = CGAffineTransform(translationX: offset.x, y: offset.y)
    }

    /// The Pocket draws the card while it flies.
    func setItemHidden(_ hidden: Bool) {
        withTransaction(Transaction(animation: nil)) {
            state.isCardHidden = hidden
        }
    }

    func setPageAlpha(_ alpha: CGFloat) {
        hosting.view.alpha = alpha
    }

    /// Restores all the flight changed, since the screen stays on the stack after it lands.
    func endFlight() {
        isInFlight = false
        view.backgroundColor = .bgSurfaceMain
        setPageAlpha(1)
        moveItem(by: .zero)
        setItemHidden(false)
    }
}

// MARK: - NavigationTransitionAnimating

extension AssetDetailsScreenViewController: NavigationTransitionAnimating {
    func navigationAnimator(for operation: UINavigationController.Operation) -> UIViewControllerAnimatedTransitioning? {
        flight(opening: operation == .push)
    }
}

// MARK: - Private

private extension AssetDetailsScreenViewController {
    /// From here on, not from when the flight starts, so the card is never drawn folded away when it flies in.
    func flight(opening: Bool) -> PocketFlight {
        isInFlight = true
        return PocketFlight(screen: self, origin: origin, opening: opening)
    }

    func close() {
        guard !isInFlight else { return }

        navigationController?.popViewController(animated: true)
    }

    func setupTitle() {
        let title = String(localized: .walletMainPurse)
        navigationItem.style = .navigator

        guard #available(iOS 26, *) else {
            navigationItem.title = title
            return
        }

        titleLabel.typography = .titleLarge
        titleLabel.attributedText = CashLogo.styled(
            title,
            symbol: PaymentAssetBranding.shared.current.symbol,
            font: UIFont.app(.titleLarge)
        )
        titleLabel.accessibilityLabel = title
        titleLabel.sizeToFit()
        navigationItem.titleView = titleLabel
    }

    func setupHosting() {
        addChild(hosting)
        view.addSubview(hosting.view)
        hosting.view.backgroundColor = .clear
        hosting.view.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
        hosting.didMove(toParent: self)
    }

    /// On iOS 26 with Reduce Transparency, the scroll edge effect that backs the bar is hidden, so the bar draws
    /// its own opaque background there.
    func applyBarBackground() {
        guard #unavailable(iOS 27), UIAccessibility.isReduceTransparencyEnabled else {
            navigationItem.standardAppearance = nil
            navigationItem.scrollEdgeAppearance = nil
            return
        }

        let appearance = (navigationController?.navigationBar.standardAppearance ?? UINavigationBarAppearance()).copy()
        appearance.configureWithOpaqueBackground()
        appearance.backgroundColor = .bgSurfaceMain
        appearance.shadowColor = nil
        navigationItem.standardAppearance = appearance
        navigationItem.scrollEdgeAppearance = appearance
    }

    @objc func reduceTransparencyDidChange() {
        applyBarBackground()
    }
}

/// The state the flight changes, which the screen's SwiftUI root draws from.
@MainActor
@Observable
private final class AssetDetailsScreenState {
    var isCardHidden = false
    @ObservationIgnored var cardFrame: CGRect = .zero
}

/// The Balance card with its details open, the details under the card.
private struct AssetDetailsScreenRoot: View {
    let viewModel: AssetDetailsViewModel
    let state: AssetDetailsScreenState

    @State private var overscroll = DSOverscroll()
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    var body: some View {
        ScrollView {
            AssetDetailsView(
                viewModel: viewModel,
                isExpanded: true,
                onCardTapped: {},
                overscroll: overscroll,
                isCardHidden: state.isCardHidden,
                onCardFrame: { state.cardFrame = $0 }
            )
            .padding(.horizontal, PocketCardSize.sideMargin)
            .padding(.top, 16)
        }
        .modifier(OverscrollReader(overscroll: overscroll))
        .modifier(OpaqueTopEdgeHidden(reduceTransparency: reduceTransparency))
    }
}

/// The host scroll view's rubber-band distance past the top. Scroll geometry is an iOS 18 API; on
/// iOS 17 the value stays 0, so an expanded header stretches with its details on a pull.
private struct OverscrollReader: ViewModifier {
    let overscroll: DSOverscroll

    func body(content: Content) -> some View {
        if #available(iOS 18, *) {
            content
                .onScrollGeometryChange(for: CGFloat.self) { geometry in
                    max(0, -(geometry.contentOffset.y + geometry.contentInsets.top))
                } action: { _, newValue in
                    overscroll.distance = newValue
                }
        } else {
            content
        }
    }
}

/// On iOS 26 with Reduce Transparency, the top scroll edge effect is drawn opaque and its height follows the
/// content, so it covers the top of the card. It is hidden there.
private struct OpaqueTopEdgeHidden: ViewModifier {
    let reduceTransparency: Bool

    func body(content: Content) -> some View {
        if #available(iOS 27, *) {
            content
        } else if #available(iOS 26, *) {
            content.scrollEdgeEffectHidden(reduceTransparency, for: .top)
        } else {
            content
        }
    }
}
