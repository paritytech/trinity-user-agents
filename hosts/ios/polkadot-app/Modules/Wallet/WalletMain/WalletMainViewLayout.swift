import SwiftUI
import PolkadotUI

struct WalletView: View {
    @State var viewModel: WalletViewModelProtocol = WalletViewModel()
    /// The funding lists the open Purse draws.
    var fundingCenter: FundingActivityCenter = .shared
    var cash: FundingCash = .current
    @State private var viewHeight: CGFloat = 0
    private let collectiblesPeekHeight: CGFloat = 90
    @State private var scrollAtTop: Bool = true
    @Namespace private var cardNamespace
    private let peekHeight: CGFloat = 64
    /// Every card in the Pocket sits one peek below the one before it, host-placed
    /// and product-backed alike, so the collection reads as a single stack.
    private var cardOverlap: CGFloat { PocketCardSize.height - peekHeight }
    private let scrollTopAnchor = "walletScrollTop"
    @State private var overscroll: CGFloat = 0
    @State private var purseCard: PurseScroll.Card?
    @State private var purseDays: [PurseScroll.DayMark] = []
    @State private var scrollOffset: CGFloat?
    @State private var navigationBar: CGRect?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ZStack(alignment: .top) {
            ScrollViewReader { scrollProxy in
                ScrollView {
                    VStack(spacing: -cardOverlap) {
                        // Builds with product cards show humanity as the
                        // Humanity card the Pocket pins, so only the others
                        // draw the ID card.
                        #if FEATURE_PRODUCTS
                            assetCard
                        #else
                            ZStack(alignment: .top) {
                                assetCard
                                identityCard
                            }
                            // The identity card is drawn offset down by the peek, which
                            // the stack's own height does not account for, so without
                            // this the product cards would start a peek too high.
                            .padding(.bottom, viewModel.expandedSection == .none ? peekHeight : 0)
                        #endif
                        pocketCards
                    }
                    .padding(.horizontal, 24)
                    .padding(.top, 16)
                    .onGeometryChange(for: CGFloat.self) { proxy in
                        proxy.size.height + proxy.safeAreaInsets.bottom
                    } action: {
                        viewHeight = $0
                    }
                    .id(scrollTopAnchor)
                }
                .modifier(OverscrollReader(overscroll: $overscroll))
                .modifier(ScrollOffsetReader(offset: $scrollOffset))
                .onChange(of: viewModel.expandedSection) { _, newValue in
                    guard newValue == .none else { return }
                    withAnimation(.spring(duration: 0.45, bounce: 0.15)) {
                        scrollProxy.scrollTo(scrollTopAnchor, anchor: .top)
                    }
                }
            }
        }
        .coordinateSpace(name: PurseScroll.space)
        .background { NavigationBarFrameReader { navigationBar = $0 } }
        .onPreferenceChange(PurseScroll.CardKey.self) { purseCard = $0 }
        .onChange(of: purseCollapse) { _, progress in viewModel.onPurseCollapse?(progress) }
        .onPreferenceChange(PurseScroll.DaysKey.self) { purseDays = $0 }
        .overlay(alignment: .top) { purseStickyBar }
        .overlay { purseHeaderCard }
        .overlay(alignment: .bottom) { purseDock }
        .modifier(
            ConditionalOverlayModifier(
                available: viewModel.isCollectiblesAvailable,
                overlay: { collectiblesCard }
            )
        )
        .animation(.spring(duration: 0.45, bounce: 0.15), value: viewModel.expandedSection)
    }

    /// Product-backed cards follow the host's own, and are hidden while a
    /// native card is expanded so they do not sit under it. The Pocket is a
    /// product surface, so it ships with the rest of them.
    @ViewBuilder
    private var pocketCards: some View {
        #if FEATURE_PRODUCTS
            if viewModel.expandedSection == .none {
                ForEach(viewModel.pocketCards) { card in
                    // A press takes the user into the product the card belongs
                    // to, at the page the card names. The shape is named so the
                    // peek a card shows is all of it that takes a press, the
                    // rest is under the card above.
                    PocketCollectionCardView(card: card)
                        .onGeometryChange(for: CGRect.self) { proxy in
                            proxy.frame(in: .global)
                        } action: {
                            viewModel.pocketCardFrames[card.key] = $0
                        }
                        // Its screen draws the card while it is open.
                        .opacity(viewModel.openPocketCard == card.key ? 0 : 1)
                        .contentShape(RoundedRectangle(cornerRadius: PocketCardSize.cornerRadius))
                        .onTapGesture { viewModel.onOpenPocketCard?(card) }
                        // A host-placed card is removable by nobody, so a long
                        // press offers it nothing rather than a prompt that
                        // refuses.
                        .onLongPressGesture {
                            guard !card.privileged else { return }

                            viewModel.onRemovePocketCard?(card)
                        }
                }
            }
        #endif
    }

    @ViewBuilder
    private var identityCard: some View {
        IdentityDetailsViewLayout(
            viewModel: viewModel.identityDetailsViewModel,
            isExpanded: viewModel.expandedSection == .identityDetails,
            onCardTapped: { viewModel.onUsername?() },
            overscroll: overscroll,
            onCollapse: { viewModel.onCollapse?() }
        )
        .matchedGeometryEffect(id: "identity", in: cardNamespace)
        .scaleEffect(identityScale)
        .opacity(identityOpacity)
        .offset(y: identityOffsetY)
        .zIndex(identityZIndex)
        .allowsHitTesting(identityHitTest)
    }

    @ViewBuilder
    private var assetCard: some View {
        AssetDetailsView(
            viewModel: viewModel.assetDetailsViewModel,
            isExpanded: viewModel.expandedSection == .assetDetails,
            onCardTapped: {
                viewModel.onBalance?()
            },
            overscroll: overscroll,
            onCollapse: { viewModel.onCollapse?() },
            cardOpacity: purseCardOpacity,
            fundingCenter: fundingCenter,
            cash: cash
        )
        .matchedGeometryEffect(id: "asset", in: cardNamespace)
        .scaleEffect(assetScale)
        .opacity(assetOpacity)
        .offset(y: assetOffsetY)
        .zIndex(assetZIndex)
        .allowsHitTesting(assetHitTest)
    }

    private var offScreenOffset: CGFloat {
        max(viewHeight, 1_000)
    }

    private var identityScale: CGFloat {
        1.0
    }

    private var identityOpacity: Double {
        switch viewModel.expandedSection {
        case .none,
             .identityDetails:
            1
        case .assetDetails,
             .collectiblesDetails:
            0
        }
    }

    private var identityOffsetY: CGFloat {
        switch viewModel.expandedSection {
        case .none:
            peekHeight
        case .identityDetails:
            0
        case .assetDetails:
            offScreenOffset
        case .collectiblesDetails:
            -offScreenOffset
        }
    }

    private var identityZIndex: Double {
        switch viewModel.expandedSection {
        case .none,
             .identityDetails:
            1
        case .assetDetails,
             .collectiblesDetails:
            0
        }
    }

    private var identityHitTest: Bool {
        switch viewModel.expandedSection {
        case .none,
             .identityDetails:
            true
        case .assetDetails,
             .collectiblesDetails:
            false
        }
    }

    private var assetScale: CGFloat {
        switch viewModel.expandedSection {
        case .none,
             .assetDetails:
            1.0
        case .identityDetails,
             .collectiblesDetails:
            0.95
        }
    }

    private var assetOpacity: Double {
        switch viewModel.expandedSection {
        case .none,
             .assetDetails:
            1
        case .identityDetails,
             .collectiblesDetails:
            0
        }
    }

    private var assetOffsetY: CGFloat {
        switch viewModel.expandedSection {
        case .none,
             .assetDetails,
             .identityDetails:
            0
        case .collectiblesDetails:
            -offScreenOffset
        }
    }

    private var assetZIndex: Double {
        switch viewModel.expandedSection {
        case .none:
            0
        case .assetDetails:
            2
        case .identityDetails,
             .collectiblesDetails:
            0
        }
    }

    private var assetHitTest: Bool {
        switch viewModel.expandedSection {
        case .none,
             .assetDetails:
            true
        case .identityDetails,
             .collectiblesDetails:
            false
        }
    }
}

private extension WalletView {
    var isPurseOpen: Bool {
        viewModel.expandedSection == .assetDetails
    }

    /// How far the open Purse has scrolled from its top.
    var purseScrollOffset: CGFloat {
        guard isPurseOpen else { return 0 }
        if let scrollOffset { return max(0, scrollOffset) }
        return purseCard.map { PurseMotion.Collapse.scrollOffset(cardTop: $0.top) } ?? 0
    }

    var purseCollapse: CGFloat {
        PurseMotion.Collapse.progress(scrollOffset: purseScrollOffset)
    }

    /// The page's card before any scroll, in the window.
    var purseRestFrame: CGRect? {
        guard let frame = purseCard?.frame, frame.width > 0 else { return nil }
        return frame.offsetBy(dx: 0, dy: purseScrollOffset)
    }

    var purseDockCentre: CGPoint? {
        navigationBar.map { CGPoint(x: $0.midX, y: $0.midY) }
    }

    /// Whether the wallet draws the card itself, shrinking into the header.
    var purseCardFloats: Bool {
        isPurseOpen && !reduceMotion && purseCollapse > 0 && purseRestFrame != nil && purseDockCentre != nil
    }

    var purseCardOpacity: Double {
        if purseCardFloats { return 0 }
        return reduceMotion ? PurseMotion.Collapse.cardOpacity(purseCollapse) : 1
    }

    @ViewBuilder
    var purseHeaderCard: some View {
        if isPurseOpen, let dock = purseDockCentre {
            if reduceMotion {
                PurseDockedThumbnail(dock: dock, opacity: PurseMotion.Collapse.thumbnailOpacity(purseCollapse))
            } else if purseCardFloats, let rest = purseRestFrame,
                      let card = viewModel.assetDetailsViewModel.balanceCardModel {
                PurseFloatingCard(
                    card: card,
                    isUpdating: viewModel.assetDetailsViewModel.isUpdating,
                    rest: rest,
                    scrollOffset: purseScrollOffset,
                    dock: dock
                )
            }
        }
    }

    @ViewBuilder
    var purseStickyBar: some View {
        if isPurseOpen, let bar = PurseScroll.bar(card: purseCard, days: purseDays) {
            PurseStickyBar(bar: bar, cash: cash)
        }
    }

    @ViewBuilder
    var purseDock: some View {
        if isPurseOpen {
            FundingInProgressDock(cash: cash, center: fundingCenter)
        }
    }
}

private extension WalletView {
    @ViewBuilder
    var collectiblesCard: some View {
        CollectiblesCardView(
            isExpanded: viewModel.expandedSection == .collectiblesDetails,
            onViewCollectibles: { viewModel.onViewCollectibles?() }
        )
        .padding(.horizontal, 24)
        .frame(maxHeight: viewModel.expandedSection == .collectiblesDetails ? .infinity : nil)
        .alignmentGuide(.bottom) { dimensions in
            switch viewModel.expandedSection {
            case .none:
                dimensions[.top] + collectiblesPeekHeight
            case .collectiblesDetails:
                dimensions[.bottom]
            case .assetDetails,
                 .identityDetails:
                dimensions[.top] - offScreenOffset
            }
        }
        .opacity(collectiblesOpacity)
        .onTapGesture { viewModel.onCollectibles?() }
        .allowsHitTesting(collectiblesHitTest)
    }

    private var collectiblesOpacity: Double {
        switch viewModel.expandedSection {
        case .none,
             .collectiblesDetails:
            1
        case .assetDetails,
             .identityDetails:
            0
        }
    }

    private var collectiblesHitTest: Bool {
        switch viewModel.expandedSection {
        case .none,
             .collectiblesDetails:
            true
        case .assetDetails,
             .identityDetails:
            false
        }
    }
}

/// The host scroll view's rubber-band distance past the top. Scroll geometry is an iOS 18 API; on
/// iOS 17 the value stays 0, so an expanded header stretches with its details on a pull.
private struct OverscrollReader: ViewModifier {
    @Binding var overscroll: CGFloat

    func body(content: Content) -> some View {
        if #available(iOS 18, *) {
            content
                .onScrollGeometryChange(for: CGFloat.self) { geometry in
                    max(0, -(geometry.contentOffset.y + geometry.contentInsets.top))
                } action: { _, newValue in
                    overscroll = newValue
                }
        } else {
            content
        }
    }
}

private struct ConditionalOverlayModifier<V: View>: ViewModifier {
    let available: Bool
    let overlay: () -> V

    func body(content: Content) -> some View {
        if available {
            content
                .overlay(alignment: .bottom) {
                    overlay()
                }
        } else {
            content
        }
    }
}
