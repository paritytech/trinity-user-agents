import Foundation
import Observation
import PolkadotUI
import Products

/// Drives the approval sheet: loads what is being offered, then stores exactly
/// that when the user approves it.
@Observable
@MainActor
final class PocketAddCardViewModel {
    enum State {
        case loading
        case offered(PocketAddCardOffer, face: CustomMessageWidgetNode?)
        case refused(String)
    }

    private(set) var state: State = .loading
    private(set) var isAdding = false

    var onFinish: () -> Void = {}

    /// How the images inside the offered face are read. The sheet shows the
    /// card as it will look, so it reads them out of the same archive the
    /// Pocket will.
    let resolveImage: WidgetImageResolver?

    private let productId: ProductId
    private let cardId: PocketCardId
    private let interactor: PocketAddCardInteractor
    private let resolver: any WidgetDesignTokenResolving

    init(
        productId: ProductId,
        cardId: PocketCardId,
        interactor: PocketAddCardInteractor,
        images: (ProductId) -> PocketImageResolver?,
        resolver: any WidgetDesignTokenResolving = WidgetDesignTokenResolver()
    ) {
        self.productId = productId
        self.cardId = cardId
        self.interactor = interactor
        self.resolver = resolver
        resolveImage = images(productId).map { images in
            WidgetImageResolver { await images.resolve($0) }
        }
    }

    func load() async {
        do {
            let offer = try await interactor.loadOffer(productId: productId, cardId: cardId)
            state = .offered(offer, face: offer.face.toWidgetNode(resolver: resolver))
        } catch {
            state = .refused(message(for: error))
        }
    }

    func add() async {
        guard case let .offered(offer, _) = state, !isAdding else { return }

        isAdding = true
        do {
            try await interactor.approve(offer)
            onFinish()
        } catch {
            isAdding = false
            state = .refused(message(for: error))
        }
    }

    /// A product that publishes no such card is told apart from one that
    /// publishes no cards at all, because the two are fixed differently.
    private func message(for error: any Error) -> String {
        switch error {
        case PocketPublishError.noPocket: String(localized: .Products.pocketDeeplinkNoPocket)
        case PocketPublishError.unknownCard: String(localized: .Products.pocketDeeplinkUnknownCard)
        default: String(localized: .Products.pocketAddCardFailed)
        }
    }
}
