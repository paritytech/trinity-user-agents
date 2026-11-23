import PolkadotUI
import Products
import SwiftUI

/// One card as the collection draws it: the face the host holds, replaced by
/// every face its product draws while the card is on screen.
struct PocketCollectionCardView: View {
    let card: PocketCardViewModel
    let pocket: ProductPocketService?
    let expanded: Bool
    let exposed: Bool

    init(card: PocketCardViewModel, pocket: ProductPocketService? = nil, expanded: Bool = false, exposed: Bool = true) {
        self.card = card
        self.pocket = pocket ?? .current
        self.expanded = expanded
        self.exposed = exposed
    }

    @State private var streamed: CustomMessageWidgetNode?

    @Environment(\.scenePhase) private var scenePhase
    @State private var inViewport = false
    @State private var appeared = false
    @State private var dismiss: (@MainActor () -> Void)?

    private var visible: Bool { exposed && scenePhase == .active && (expanded || inViewport) }
    private var drawing: DrawingKey {
        DrawingKey(cardId: card.id, installation: pocket?.installation ?? 0, visible: visible)
    }

    private let resolver = WidgetDesignTokenResolver()

    var body: some View {
        PocketProductCardView(
            title: card.title,
            face: streamed ?? card.face,
            resolveImage: pocket?.images(of: card.key.productId).map { [privileged = card.privileged] images in
                WidgetImageResolver.cached { source in
                    if privileged, case let .archive(path) = source,
                       let shipped = AssetPinnedPocketCards.image(at: path) {
                        return shipped
                    }
                    return await images.resolve(source)
                }
            },
            onAction: { action, value in send(action, value) }
        )
        .onGeometryChange(for: Bool.self) { proxy in
            guard let viewport = proxy.bounds(of: .scrollView) else { return false }
            return viewport.intersects(CGRect(origin: .zero, size: proxy.size))
        } action: { inViewport = $0 }
        .onAppear {
            appeared = true
            updatePresentation()
        }
        .onChange(of: drawing) { if appeared { updatePresentation() } }
        .onDisappear {
            appeared = false
            dismiss?()
            dismiss = nil
        }
        .task(id: drawing) {
            if visible { await draw() }
        }
    }

    private func updatePresentation() {
        let previous = dismiss
        if visible, let pocket {
            dismiss = pocket.present(card.key, expanded: expanded)
        } else {
            dismiss = nil
        }
        previous?()
    }

    /// What a card's drawing task is keyed on: the card, and the Pocket it is
    /// being drawn from.
    private struct DrawingKey: Equatable {
        let cardId: String
        let installation: Int
        let visible: Bool
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
