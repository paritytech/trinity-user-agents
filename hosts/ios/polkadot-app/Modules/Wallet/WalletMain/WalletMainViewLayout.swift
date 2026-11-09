import SwiftUI
import PolkadotUI
import Products

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

    static let sectionAnimationDuration: TimeInterval = 0.45
    static var sectionAnimation: Animation { .spring(duration: sectionAnimationDuration, bounce: 0.15) }

    var body: some View {
        ZStack(alignment: .top) {
            ScrollViewReader { scrollProxy in
                ScrollView {
                    VStack(spacing: -cardOverlap) {
                        assetCard
                            .pocketMotion(viewModel.isPocketCleared ? .shrunkAway : .resting)
                        pocketCards
                    }
                    .padding(.horizontal, PocketCardSize.sideMargin)
                    .padding(.top, 16)
                    .onGeometryChange(for: CGFloat.self) { proxy in
                        proxy.size.height + proxy.safeAreaInsets.bottom
                    } action: {
                        viewHeight = $0
                    }
                    .id(scrollTopAnchor)
                }
                .scrollClipDisabled(viewModel.openPocketCard != nil)
                .modifier(TopScrollEdgeEffectHidden(hidden: viewModel.openPocketCard != nil))
                .modifier(OverscrollReader(overscroll: $overscroll))
                .onChange(of: viewModel.expandedSection) { _, newValue in
                    guard newValue == .none else { return }
                    withAnimation(Self.sectionAnimation) {
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
        .animation(Self.sectionAnimation, value: viewModel.expandedSection)
    }

    /// Product-backed cards follow the host's own, and are hidden while a
    /// native card is expanded so they do not sit under it.
    @ViewBuilder
    private var pocketCards: some View {
        if viewModel.expandedSection == .none {
            let openIndex = viewModel.pocketCards.firstIndex { $0.key == viewModel.openPocketCard }
            ForEach(Array(viewModel.pocketCards.enumerated()), id: \.element.id) { index, card in
                // A press takes the user into the product the card belongs
                // to, at the page the card names. The shape is named so the
                // peek a card shows is all of it that takes a press, the
                // rest is under the card above.
                PocketCollectionCardView(card: card)
                    .pocketMotion(motion(ofCardAt: index, openAt: openIndex))
                    .opacity(viewModel.expandedSection == .none ? 1 : 0)
                    .allowsHitTesting(viewModel.expandedSection == .none)
                    .onGeometryChange(for: CGRect.self) { proxy in
                        proxy.frame(in: .global)
                    } action: {
                        viewModel.pocketCardFrames[card.key] = $0
                    }
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
    }

    private func motion(ofCardAt index: Int, openAt openIndex: Int?) -> PocketCardMotion {
        PocketCardMotion.of(
            cardAt: index,
            openAt: openIndex,
            openOffset: viewModel.openPocketCardOffset,
            cleared: viewModel.isPocketCleared
        )
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
        .zIndex(assetZIndex)
    }

    private var offScreenOffset: CGFloat {
        max(viewHeight, 1_000)
    }

    private var assetZIndex: Double {
        switch viewModel.expandedSection {
        case .none:
            0
        case .assetDetails:
            2
        }
    }
}

private extension WalletView {
    @ViewBuilder
    var collectiblesCard: some View {
        Button {
            viewModel.onCollectibles?()
        } label: {
            CollectiblesCardView(isColoured: viewModel.collectiblesPeek != .resting)
        }
        .buttonStyle(CollectiblesPressStyle())
        .accessibilityLabel(String(localized: .Products.collectibles))
        .onGeometryChange(for: CGRect.self) { proxy in
            proxy.frame(in: .global)
        } action: {
            viewModel.collectiblesFrame = $0
        }
        .padding(.horizontal, PocketCardSize.sideMargin)
        .alignmentGuide(.bottom) { dimensions in
            switch viewModel.expandedSection {
            case .none:
                dimensions[.top] + collectiblesPeekHeight
            case .assetDetails:
                dimensions[.top] - offScreenOffset
            }
        }
        .opacity(viewModel.isPocketCleared || viewModel.collectiblesPeek == .lifted ? 0 : collectiblesOpacity)
        .allowsHitTesting(collectiblesHitTest)
    }

    private var collectiblesOpacity: Double {
        switch viewModel.expandedSection {
        case .none:
            1
        case .assetDetails:
            0
        }
    }

    private var collectiblesHitTest: Bool {
        switch viewModel.expandedSection {
        case .none:
            true
        case .assetDetails:
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

/// A card lifted out of the Pocket can sit under the title bar, where iOS 26
/// would darken it with the scroll view's top edge effect.
private struct TopScrollEdgeEffectHidden: ViewModifier {
    let hidden: Bool

    func body(content: Content) -> some View {
        if #available(iOS 26, *) {
            content.scrollEdgeEffectHidden(hidden, for: .top)
        } else {
            content
        }
    }
}

private extension View {
    /// Applied inside any geometry reader, so a card's slot stays where layout puts it.
    func pocketMotion(_ motion: PocketCardMotion) -> some View {
        offset(motion.offset)
            .scaleEffect(motion.scale)
            .opacity(motion.opacity)
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

/// The Collectibles peek shrinks a little while it is pressed, as Android's does.
private struct CollectiblesPressStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? CollectiblesBookTiming.pressedScale : 1)
            .animation(.easeOut(duration: CollectiblesBookTiming.pressDuration), value: configuration.isPressed)
    }
}
