import PolkadotUI
import SwiftUI
import enum TrUAPIHost.PocketCardSide

/// The card as an opened screen draws it: the frame the collection gives it,
/// with the same margins the collection leaves around it.
///
/// Both faces are drawn, the one turned away waiting behind, so a turn shows
/// it at once. Only a card with a back turns.
struct PocketOpenedCardView: View {
    static let verticalInset: CGFloat = 16

    static var height: CGFloat { PocketCardSize.height + verticalInset * 2 }

    let card: PocketCardViewModel
    /// Read only: the screen showing the card is the one that turns it.
    let turn: PocketCardTurn
    let onBackChanged: @MainActor (_ available: Bool) -> Void
    let onTurnOver: @MainActor () -> Void
    let pocket: ProductPocketService? = .current

    @State private var back: CustomMessageWidgetNode?

    private let resolver = WidgetDesignTokenResolver()

    var body: some View {
        ZStack {
            PocketCardEdge(turn: turn)
            PocketCollectionCardView(card: card)
                .modifier(TurnedFace(turn: turn, side: .front, depth: -PocketCardEdge.thickness / 2))
            PocketProductCardView(
                title: card.title,
                face: back,
                resolveImage: pocket?.faceImages(of: card),
                onAction: { action, value in pocket?.send(action, value, for: card.key, side: .back) }
            )
            .modifier(TurnedFace(turn: turn, side: .back, depth: PocketCardEdge.thickness / 2))
        }
        .accessibilityElement(children: .contain)
        .accessibilityActions {
            if turn.canTurn {
                Button(String(localized: .Products.pocketCardTurnOver)) { onTurnOver() }
            }
        }
        .padding(.horizontal, PocketCardSize.sideMargin)
        .padding(.vertical, Self.verticalInset)
        .task(id: PocketCollectionCardView.DrawingKey(cardId: card.id, installation: pocket?.installation ?? 0)) {
            await drawBack()
        }
    }

    private func drawBack() async {
        guard let pocket else { return }

        for await node in pocket.backFaces(for: card.key) {
            back = node?.toWidgetNode(resolver: resolver)
            onBackChanged(back != nil)
        }
    }
}

/// The card's thickness: strips along its side nearer the viewer, stacked between the faces. Turning about the
/// vertical axis never shows the top, bottom or far edges.
private struct PocketCardEdge: View {
    static let thickness: CGFloat = 4
    private static let slices = 6
    private static let strip: CGFloat = 24
    private static let shaded = (red: 0x16, green: 0x17, blue: 0x1A)
    private static let lit = (red: 0x4A, green: 0x4C, blue: 0x53)

    let turn: PocketCardTurn

    var body: some View {
        ZStack {
            ForEach(0 ..< Self.slices, id: \.self) { slice in
                RoundedRectangle(cornerRadius: PocketCardSize.cornerRadius)
                    .fill(color)
                    .mask(alignment: turn.isLeftSideNearer ? .leading : .trailing) {
                        Rectangle().frame(width: Self.strip)
                    }
                    .modifier(TurnedLayer(
                        turn: turn,
                        side: .front,
                        depth: Self.thickness * (Double(slice) / Double(Self.slices - 1) - 0.5)
                    ))
            }
        }
        .frame(height: PocketCardSize.height)
        .accessibilityHidden(true)
    }

    /// The edge catches more light the more of it faces the viewer.
    private var color: Color {
        func channel(_ shaded: Int, _ lit: Int) -> Double {
            (Double(shaded) + (Double(lit) - Double(shaded)) * turn.sideOn) / 255
        }

        return Color(
            red: channel(Self.shaded.red, Self.lit.red),
            green: channel(Self.shaded.green, Self.lit.green),
            blue: channel(Self.shaded.blue, Self.lit.blue)
        )
    }
}

/// Draws a layer of the card `depth` points behind its middle, as `side` is seen at the current turn. The angle is
/// read here rather than by the card, so a turn redraws the layers and not the faces inside them.
private struct TurnedLayer: ViewModifier {
    let turn: PocketCardTurn
    let side: PocketCardSide
    let depth: Double

    func body(content: Content) -> some View {
        content
            .rotation3DEffect(
                .degrees(turn.layerRotation(side)),
                axis: (x: 0, y: 1, z: 0),
                perspective: 0.42
            )
            .scaleEffect(turn.scale)
            .offset(x: turn.depthOffset(depth))
    }
}

/// Only the face up takes presses and is read aloud, judged from where the card is mid-turn rather than where it is
/// going.
private struct TurnedFace: ViewModifier {
    let turn: PocketCardTurn
    let side: PocketCardSide
    let depth: Double

    func body(content: Content) -> some View {
        let shown = turn.shownSide == side

        return content
            .modifier(TurnedLayer(turn: turn, side: side, depth: depth))
            .opacity(shown ? 1 : 0)
            .zIndex(shown ? 1 : 0)
            .allowsHitTesting(shown)
            .accessibilityHidden(!shown)
    }
}
