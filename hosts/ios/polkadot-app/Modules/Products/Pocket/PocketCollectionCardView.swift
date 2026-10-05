import PolkadotUI
import Products
import SwiftUI

/// One card as the collection draws it: the face the host holds, replaced by
/// every face its product draws while the card is on screen.
struct PocketCollectionCardView: View {
    let card: PocketCardViewModel
    let pocket: ProductPocketService?

    init(card: PocketCardViewModel, pocket: ProductPocketService? = nil) {
        self.card = card
        self.pocket = pocket ?? .current
    }

    @State private var streamed: CustomMessageWidgetNode?

    private let resolver = WidgetDesignTokenResolver()

    var body: some View {
        PocketProductCardView(
            title: card.title,
            face: streamed ?? card.face,
            resolveImage: pocket?.images(of: card.key.productId).map { images in
                WidgetImageResolver { await images.resolve($0) }
            },
            onAction: { action, value in send(action, value) }
        )
        // Keyed on the Pocket as well as the card: a session that installed a
        // new one ended the stream this card was drawing, and a key of its own
        // id alone would never start another.
        .task(id: DrawingKey(cardId: card.id, installation: pocket?.installation ?? 0)) { await draw() }
    }

    /// What a card's drawing task is keyed on: the card, and the Pocket it is
    /// being drawn from.
    private struct DrawingKey: Equatable {
        let cardId: String
        let installation: Int
    }

    private func draw() async {
        guard let pocket else { return }

        for await face in pocket.faces(for: card.key) {
            streamed = face.toWidgetNode(resolver: resolver)
        }
    }

    /// A press carries no payload; a text edit carries the new value as UTF-8,
    /// which is the shape every host sends.
    private func send(_ action: String, _ value: String?) {
        pocket?.send(action: action, payload: Data((value ?? "").utf8), for: card.key)
    }
}
