import SwiftUI
import PolkadotUI

struct WalletView: View {
    @State var viewModel: WalletViewModelProtocol = WalletViewModel()
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

    var body: some View {
        ZStack(alignment: .top) {
            ScrollViewReader { scrollProxy in
                ScrollView {
                    VStack(spacing: -cardOverlap) {
                        ZStack(alignment: .top) {
                            assetCard
                            identityCard
                        }
                        // The identity card is drawn offset down by the peek, which
                        // the stack's own height does not account for, so without
                        // this the product cards would start a peek too high.
                        .padding(.bottom, viewModel.expandedSection == .none ? peekHeight : 0)
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
                .safeAreaInset(edge: .bottom) {
                    if viewModel.expandedSection == .assetDetails {
                        AssetDetailsFundingBar(viewModel: viewModel.assetDetailsViewModel)
                    }
                }
                .modifier(OverscrollReader(overscroll: $overscroll))
                .onChange(of: viewModel.expandedSection) { _, newValue in
                    guard newValue == .none else { return }
                    withAnimation(.spring(duration: 0.45, bounce: 0.15)) {
                        scrollProxy.scrollTo(scrollTopAnchor, anchor: .top)
                    }
                }
            }
        }
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
    private var collectiblesCard: some View {
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
            onCollapse: { viewModel.onCollapse?() }
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
