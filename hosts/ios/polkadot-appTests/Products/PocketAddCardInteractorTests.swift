import Foundation
import Products
import Testing
@testable import polkadot_app

/// Loads what the user is being asked to approve, and stores exactly that.
///
/// What a product publishes is `PublishedPocketCards`' answer and is proved in
/// its own suite. What is left here is the interactor's own work: turning an
/// answer into an offer, and keeping the face the offer was approved by.
struct PocketAddCardInteractorTests {
    @Test
    func offersThePublishedCard() async throws {
        let interactor = makeAddCardInteractor(published: [loyaltyDefinition])

        let offer = try await interactor.loadOffer(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))

        #expect(offer.title == "Loyalty")
        #expect(offer.productName == "Game")
        #expect(offer.key.cardId.value == "loyalty")
    }

    /// A refusal is the card lookup's answer, so it reaches the sheet as it
    /// came rather than as something the interactor made up.
    @Test
    func raisesTheRefusalTheCardLookupGave() async {
        let interactor = makeAddCardInteractor(published: [loyaltyDefinition], includesPocket: false)

        await #expect(throws: PocketPublishError.noPocket) {
            try await interactor.loadOffer(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))
        }
    }

    /// The face the user approves is the one that is stored: approving keeps
    /// the offer as loaded rather than re-reading anything.
    @Test
    func approvingAddsTheCardAsUnprivileged() async throws {
        let store = InMemoryPocketCardStore()
        let interactor = makeAddCardInteractor(published: [loyaltyDefinition], store: store)
        let offer = try await interactor.loadOffer(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))

        try await interactor.approve(offer)

        #expect(try await store.cards() == [
            PocketCardEntry(key: offer.key, title: "Loyalty", privileged: false)
        ])
        #expect(await store.face(for: offer.key) == offer.face)
    }

    /// A card the store could not keep is raised rather than reported as
    /// approved: the sheet decides what to show, and it cannot decide from a
    /// success that did not happen.
    @Test
    func raisesWhenTheCardCouldNotBeStored() async throws {
        let store = InMemoryPocketCardStore()
        await store.failWrites()
        let interactor = makeAddCardInteractor(published: [loyaltyDefinition], store: store)
        let offer = try await interactor.loadOffer(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))

        await #expect(throws: InMemoryPocketCardStore.Unavailable.self) {
            try await interactor.approve(offer)
        }
    }
}

// MARK: - Fixtures

private let loyaltyDefinition = PocketCardDefinition(
    id: PocketCardId(value: "loyalty"),
    title: "Loyalty",
    preview: .archive(path: "faces/loyalty.json")
)
