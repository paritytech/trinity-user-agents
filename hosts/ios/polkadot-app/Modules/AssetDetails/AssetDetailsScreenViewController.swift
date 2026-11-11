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
    private let cash: FundingCash
    private let state = AssetDetailsScreenState()
    private lazy var hosting = UIHostingController(
        rootView: AssetDetailsScreenRoot(viewModel: viewModel, state: state, cash: cash)
    )
    private lazy var pullDown = PocketPullDown(screen: self) { [weak self] in self?.close() }
    private let titleLabel: PolkadotUI.Label = .create { view in
        view.typography = .headlineSmall
        view.textColor = .fgPrimary
    }

    private let purseTitleView = PurseTitleView()

    private var isInFlight = false

    init(viewModel: AssetDetailsViewModel, origin: PocketOrigin?, cash: FundingCash) {
        self.viewModel = viewModel
        self.origin = origin
        self.cash = cash

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
        setupHistoryButton()
        state.onCollapse = { [weak self] progress in self?.fadeTitle(progress: progress) }
        setupHosting()
        pullDown.install()
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
            navigationItem.titleView = purseTitleView
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

    func setupHistoryButton() {
        let item = UIBarButtonItem(
            image: UIImage(systemName: "clock.arrow.circlepath"),
            primaryAction: UIAction { [weak self] _ in self?.viewModel.onHistory?() }
        )
        item.accessibilityLabel = String(localized: .Funding.historyOpen)
        navigationItem.rightBarButtonItem = item
    }

    /// The title gives way as the card shrinks into the header in its place.
    func fadeTitle(progress: CGFloat) {
        let opacity = PurseMotion.Collapse.titleOpacity(progress)
        titleLabel.alpha = opacity
        purseTitleView.apply(progress: progress)
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
}

/// The state the flight changes, which the screen's SwiftUI root draws from.
@MainActor
@Observable
private final class AssetDetailsScreenState {
    var isCardHidden = false
    @ObservationIgnored var cardFrame: CGRect = .zero
    /// How far the card has shrunk into the header, 0 to 1.
    @ObservationIgnored var onCollapse: ((CGFloat) -> Void)?
}

/// The Balance card with its details open, the details under the card. As the details scroll up, the card
/// shrinks into the header and a bar with the current balance takes its place.
private struct AssetDetailsScreenRoot: View {
    let viewModel: AssetDetailsViewModel
    let state: AssetDetailsScreenState
    let cash: FundingCash

    @State private var overscroll = DSOverscroll()
    @State private var card: PurseScroll.Card?
    @State private var scrollOffset: CGFloat?
    @State private var navigationBar: CGRect?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ScrollView {
            AssetDetailsView(
                viewModel: viewModel,
                isExpanded: true,
                onCardTapped: {},
                overscroll: overscroll,
                isCardHidden: state.isCardHidden,
                onCardFrame: { state.cardFrame = $0 },
                cardOpacity: cardOpacity,
                cash: cash
            )
            .padding(.horizontal, PocketCardSize.sideMargin)
            .padding(.top, 16)
        }
        .modifier(OverscrollReader(overscroll: overscroll))
        .modifier(ScrollOffsetReader(offset: $scrollOffset))
        .coordinateSpace(name: PurseScroll.space)
        .background { NavigationBarFrameReader { navigationBar = $0 } }
        .onPreferenceChange(PurseScroll.CardKey.self) { card = $0 }
        .onChange(of: collapse) { _, progress in state.onCollapse?(progress) }
        .overlay(alignment: .top) { stickyBar }
        .overlay { headerCard }
    }
}

private extension AssetDetailsScreenRoot {
    /// How far the details have scrolled from their top.
    var offset: CGFloat {
        if let scrollOffset { return max(0, scrollOffset) }
        return card.map { PurseMotion.Collapse.scrollOffset(cardTop: $0.top) } ?? 0
    }

    var collapse: CGFloat {
        PurseMotion.Collapse.progress(scrollOffset: offset)
    }

    /// The page's card before any scroll, in the window.
    var restFrame: CGRect? {
        guard let frame = card?.frame, frame.width > 0 else { return nil }
        return frame.offsetBy(dx: 0, dy: offset)
    }

    var dockCentre: CGPoint? {
        navigationBar.map { CGPoint(x: $0.midX, y: $0.midY) }
    }

    /// Whether the screen draws the card itself, shrinking into the header. Never while the Pocket draws it in
    /// flight.
    var cardFloats: Bool {
        !state.isCardHidden && !reduceMotion && collapse > 0 && restFrame != nil && dockCentre != nil
    }

    var cardOpacity: Double {
        if cardFloats { return 0 }
        return reduceMotion ? PurseMotion.Collapse.cardOpacity(collapse) : 1
    }

    @ViewBuilder
    var headerCard: some View {
        if !state.isCardHidden, let dock = dockCentre {
            if reduceMotion {
                PurseDockedThumbnail(dock: dock, opacity: PurseMotion.Collapse.thumbnailOpacity(collapse))
            } else if cardFloats, let rest = restFrame, let model = viewModel.balanceCardModel {
                PurseFloatingCard(
                    card: model,
                    isUpdating: viewModel.isUpdating,
                    rest: rest,
                    scrollOffset: offset,
                    dock: dock
                )
            }
        }
    }

    @ViewBuilder
    var stickyBar: some View {
        if let bar = PurseScroll.bar(card: card) {
            PurseStickyBar(bar: bar, cash: cash)
        }
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
