import Foundation
import Products
import Testing
@testable import polkadot_app

struct PublishedPocketCardsTests {
    @Test
    func findsACardTheWorkerPublishes() async throws {
        let cards = PublishedPocketCards(products: gameResolver(worker: workerPublishing([loyalty])))

        let found = try await cards.find(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))

        #expect(found.definition == loyalty)
        #expect(found.productName == "Game")
    }

    /// The face lives in the worker's own archive, which resolves under the
    /// worker's subname rather than the product's base name.
    @Test
    func namesTheWorkerArchiveTheFaceIsReadFrom() async throws {
        let cards = PublishedPocketCards(products: gameResolver(worker: workerPublishing([loyalty])))

        let found = try await cards.find(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))

        #expect(found.workerContentId == "worker.game.paseo")
    }

    /// A deeplink may name the product in any casing dotNS accepts, and the
    /// collection is keyed by one id only, the one the resolver settled on.
    @Test
    func keysTheCardByTheResolvedProductIdRatherThanTheOneAskedFor() async throws {
        let cards = PublishedPocketCards(products: gameResolver(worker: workerPublishing([loyalty])))

        let found = try await cards.find(productId: "GAME.paseo", cardId: PocketCardId(value: "loyalty"))

        #expect(found.productId == "game.paseo")
    }

    @Test
    func refusesAProductWithNoWorker() async {
        let cards = PublishedPocketCards(products: gameResolver(worker: nil))

        await #expect(throws: PocketPublishError.noPocket) {
            try await cards.find(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))
        }
    }

    /// Cards come with the Pocket a worker declares, so a worker that declares
    /// only chat publishes none.
    @Test
    func refusesAWorkerThatDeclaresNoPocket() async {
        let worker = ProductExecutable.Worker(
            identifier: "worker.game.paseo",
            appVersion: .zero,
            entrypoint: "worker.js",
            modalities: [.chat]
        )
        let cards = PublishedPocketCards(products: gameResolver(worker: worker))

        await #expect(throws: PocketPublishError.noPocket) {
            try await cards.find(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))
        }
    }

    // MARK: - Debug cards

    /// A published worker always wins: a debug card must never shadow what a
    /// product actually publishes, or a test would pass against a face the
    /// product never shipped.
    @Test
    func prefersThePublishedCardOverADebugOne() async throws {
        let cards = PublishedPocketCards(
            products: gameResolver(worker: workerPublishing([loyalty])),
            debugCards: { _ in [debugLoyalty] }
        )

        let found = try await cards.find(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))

        #expect(found.definition.preview == .archive(path: "faces/loyalty.json"))
    }

    /// A product with no published worker is how a card is driven before it is
    /// published at all, which is the only way the face URL is ever used.
    @Test
    func fallsBackToADebugCardWhenNothingIsPublished() async throws {
        let cards = PublishedPocketCards(
            products: gameResolver(worker: nil),
            debugCards: { _ in [debugLoyalty] }
        )

        let found = try await cards.find(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))

        #expect(found.definition.preview == .url("http://127.0.0.1:5173/pocket/devicehood.json"))
    }

    // MARK: - When the product cannot be read

    /// A malformed manifest is a settled answer, not a read that did not land.
    /// Left transient, the card asks the chain again every thirty seconds for
    /// as long as it is on screen, and never draws.
    @Test
    func refusesAProductWhoseManifestCannotBeRead() async {
        let cards = PublishedPocketCards(products: StubProductResolver(
            alwaysFailing: ProductResolutionError.malformedManifest("game.paseo")
        ))

        await #expect(throws: PocketPublishError.unreadableProduct) {
            try await cards.find(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))
        }
    }

    /// A chain read that did not land is not an answer, so it is raised as
    /// itself and the caller asks again.
    @Test
    func raisesAReadThatDidNotLandAsItself() async {
        let cards = PublishedPocketCards(products: StubProductResolver(alwaysFailing: URLError(.timedOut)))

        await #expect(throws: URLError.self) {
            try await cards.find(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))
        }
    }

    /// A card typed in by hand needs no chain presence, so it stands even while
    /// the product cannot be read at all.
    @Test
    func stillServesAHandTypedCardWhenTheProductCannotBeRead() async throws {
        let cards = PublishedPocketCards(
            products: StubProductResolver(alwaysFailing: URLError(.timedOut)),
            debugCards: { _ in [debugLoyalty] }
        )

        let found = try await cards.find(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))

        #expect(found.definition.preview == .url("http://127.0.0.1:5173/pocket/devicehood.json"))
    }

    @Test
    func refusesACardTheWorkerDoesNotPublish() async {
        let cards = PublishedPocketCards(products: gameResolver(worker: workerPublishing([loyalty])))

        await #expect(throws: PocketPublishError.unknownCard) {
            try await cards.find(productId: "game.paseo", cardId: PocketCardId(value: "trophy"))
        }
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardDefinition(
    id: PocketCardId(value: "loyalty"),
    title: "Loyalty",
    preview: .archive(path: "faces/loyalty.json")
)

private let debugLoyalty = PocketCardDefinition(
    id: PocketCardId(value: "loyalty"),
    title: "Loyalty (debug)",
    preview: .url("http://127.0.0.1:5173/pocket/devicehood.json")
)

private func workerPublishing(_ cards: [PocketCardDefinition]) -> ProductExecutable.Worker {
    ProductExecutable.Worker(
        identifier: "worker.game.paseo",
        appVersion: .zero,
        entrypoint: "worker.js",
        modalities: [.pocket(cards)]
    )
}

/// Answers any id with the one product these tests ask about, including the mixed-casing id one
/// test deliberately asks under.
private func gameResolver(worker: ProductExecutable.Worker?) -> StubProductResolver {
    StubProductResolver(alwaysResolvingTo: ResolvedProduct(
        id: "game.paseo",
        displayName: "Game",
        description: nil,
        icon: nil,
        executables: ProductExecutables(app: nil, widget: nil, worker: worker),
        hasManifest: true
    ))
}
