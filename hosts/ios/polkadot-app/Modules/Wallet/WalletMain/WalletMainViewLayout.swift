import SwiftUI
import PolkadotUI
import Products

struct WalletView: View {
    @State var viewModel: WalletViewModelProtocol = WalletViewModel()
    private let collectiblesPeekHeight: CGFloat = 90
    @State private var scrollAtTop: Bool = true
    @Namespace private var cardNamespace
    private let peekHeight: CGFloat = 64
    /// Every card in the Pocket sits one peek below the one before it, host-placed
    /// and product-backed alike, so the collection reads as a single stack.
    private var cardOverlap: CGFloat { PocketCardSize.height - peekHeight }

    var body: some View {
        ZStack(alignment: .top) {
            ScrollView {
                VStack(spacing: -cardOverlap) {
                    assetCard
                        .pocketMotion(motion(ofPosition: 0))
                        .onGeometryChange(for: CGRect.self) { proxy in
                            proxy.frame(in: .global)
                        } action: {
                            viewModel.balanceCardFrame = $0
                        }
                    pocketCards
                }
                .padding(.horizontal, PocketCardSize.sideMargin)
                .padding(.top, 16)
            }
            .scrollClipDisabled(isCardOpen)
            .modifier(TopScrollEdgeEffect(hidden: isCardOpen))
        }
        .modifier(
            ConditionalOverlayModifier(
                available: viewModel.isCollectiblesAvailable,
                overlay: { collectiblesCard }
            )
        )
    }

    /// Product-backed cards follow the host's own.
    @ViewBuilder
    private var pocketCards: some View {
        ForEach(Array(viewModel.pocketCards.enumerated()), id: \.element.id) { index, card in
            // A press takes the user into the product the card belongs
            // to, at the page the card names. The shape is named so the
            // peek a card shows is all of it that takes a press, the
            // rest is under the card above.
            PocketCollectionCardView(card: card)
                .pocketMotion(motion(ofPosition: index + 1))
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

    @ViewBuilder
    private var assetCard: some View {
        AssetDetailsView(
            viewModel: viewModel.assetDetailsViewModel,
            onCardTapped: {
                viewModel.onBalance?()
            },
            // The open flight clears the Pocket on its first frame, so the face switches as the card lifts.
            // The close flight restores the Pocket as it starts, so the card turns back before it flies home.
            showsOpenedFace: viewModel.openItem == .balance && viewModel.isPocketCleared
        )
        .matchedGeometryEffect(id: "asset", in: cardNamespace)
    }
}

private extension WalletView {
    var isCardOpen: Bool {
        switch viewModel.openItem {
        case .balance,
             .card:
            true
        case .collectibles,
             nil:
            false
        }
    }

    /// The position of the open item in the stack: the Balance card is first, and each Pocket card follows it.
    var openPosition: Int? {
        switch viewModel.openItem {
        case .balance:
            0
        case let .card(key):
            viewModel.pocketCards.firstIndex { $0.key == key }.map { $0 + 1 }
        case .collectibles,
             nil:
            nil
        }
    }

    func motion(ofPosition position: Int) -> PocketCardMotion {
        PocketCardMotion.of(
            cardAt: position,
            openAt: openPosition,
            openOffset: viewModel.openPocketCardOffset,
            cleared: viewModel.isPocketCleared
        )
    }

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
            dimensions[.top] + collectiblesPeekHeight
        }
        .opacity(viewModel.isPocketCleared || viewModel.collectiblesPeek == .lifted ? 0 : 1)
    }
}

/// The hard style keeps the status strip title readable over the cards. A card lifted out of the Pocket
/// can sit under the title bar, where the effect would darken it, so it is hidden then.
private struct TopScrollEdgeEffect: ViewModifier {
    let hidden: Bool

    func body(content: Content) -> some View {
        if #available(iOS 26, *) {
            content
                .scrollEdgeEffectStyle(.hard, for: .top)
                .scrollEdgeEffectHidden(hidden, for: .top)
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
        content
            .overlay(alignment: .bottom) {
                overlay()
                    .opacity(available ? 1 : 0)
                    .allowsHitTesting(available)
                    .accessibilityHidden(!available)
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
