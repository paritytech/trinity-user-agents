import PolkadotUI
import Products
import SwiftUI
import enum TrUAPIHost.PocketCardSide

/// One card as the collection draws it: the face the host holds, replaced by
/// every face its product draws while the card is on screen.
struct PocketCollectionCardView: View {
    let card: PocketCardViewModel
    let pocket: ProductPocketService?
    /// A privileged card's back is kept current while it is listed, so it is
    /// ready before the card is opened.
    let keepsBackCurrent: Bool

    init(card: PocketCardViewModel, pocket: ProductPocketService? = nil, keepsBackCurrent: Bool = false) {
        self.card = card
        self.pocket = pocket ?? .current
        self.keepsBackCurrent = keepsBackCurrent
    }

    @State private var streamed: CustomMessageWidgetNode?

    private let resolver = WidgetDesignTokenResolver()

    var body: some View {
        PocketProductCardView(
            title: card.title,
            face: streamed ?? card.face,
            resolveImage: pocket?.faceImages(of: card),
            onAction: { action, value in pocket?.send(action, value, for: card.key, side: .front) }
        )
        // Keyed on the Pocket as well as the card: a session that installed a
        // new one ended the stream this card was drawing, and a key of its own
        // id alone would never start another.
        .task(id: DrawingKey(cardId: card.id, installation: pocket?.installation ?? 0)) { await draw() }
    }

    /// What a card's drawing task is keyed on: the card, and the Pocket it is
    /// being drawn from.
    struct DrawingKey: Equatable {
        let cardId: String
        let installation: Int
    }

    private func draw() async {
        guard let pocket else { return }

        await withDiscardingTaskGroup { group in
            if keepsBackCurrent {
                group.addTask { for await _ in pocket.backFaces(for: card.key) {} }
            }
            for await face in pocket.faces(for: card.key) {
                streamed = face.toWidgetNode(resolver: resolver)
            }
        }
    }
}

extension ProductPocketService {
    /// How the images inside a card's faces are read: a privileged card's
    /// shipped images first, then its product's archive.
    func faceImages(of card: PocketCardViewModel) -> WidgetImageResolver? {
        images(of: card.key.productId).map { [privileged = card.privileged] images in
            WidgetImageResolver.cached { source in
                if privileged, case let .archive(path) = source,
                   let shipped = AssetPinnedPocketCards.image(at: path) {
                    return shipped
                }
                return await images.resolve(source)
            }
        }
    }

    /// A press carries no payload; a text edit carries the new value as UTF-8,
    /// which is the shape every host sends.
    func send(_ action: String, _ value: String?, for key: PocketCardKey, side: PocketCardSide) {
        send(action: action, payload: Data((value ?? "").utf8), for: key, side: side)
    }
}
