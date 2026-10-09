import Foundation
import Products
import Testing
import UIKit
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

    // MARK: - The face a card opens with

    @Test
    func opensACardWithItsFaceAwayWhenItsProductPublishesItSo() async {
        let cards = PublishedPocketCards(products: gameResolver(worker: workerPublishing([faceAwayLoyalty])))

        let faceShown = await PocketCardFaceOnOpen.faceShown(for: loyaltyKey, cards: cards)

        #expect(!faceShown)
    }

    /// Nothing asks for the face away of a card its product does not publish,
    /// and a face hidden by mistake is not one the user knows to pull back.
    @Test
    func opensACardItsProductDoesNotPublishWithItsFace() async {
        let cards = PublishedPocketCards(products: gameResolver(worker: workerPublishing([faceAwayLoyalty])))
        let trophy = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "trophy"))

        let faceShown = await PocketCardFaceOnOpen.faceShown(for: trophy, cards: cards)

        #expect(faceShown)
    }

    /// A chain read that hangs must not fold the face long after the card
    /// opened: an answer past the bound leaves the face shown.
    @Test
    func opensWithItsFaceWhenTheProductDoesNotAnswerInTime() async {
        let late = SlowProductResolver(
            product: gameProduct(worker: workerPublishing([faceAwayLoyalty])),
            delay: .milliseconds(100)
        )
        let cards = PublishedPocketCards(products: late)
        let started = ContinuousClock.now

        let faceShown = await PocketCardFaceOnOpen.faceShown(for: loyaltyKey, cards: cards, timeout: .milliseconds(10))

        #expect(faceShown)
        #expect(ContinuousClock.now - started < .seconds(5))
    }

    /// The card is shown before its product is asked, so an answer that comes
    /// once the card is on screen must still fold the face away.
    @Test @MainActor
    func foldsTheFaceWhenTheProductAnswersAfterTheCardIsShown() async throws {
        let screen = PocketCardScreenViewController(
            card: loyaltyCard,
            product: StubSPAView(),
            surface: PocketCardSurface()
        )
        let presenter = UIViewController()
        let window = try showingInScene(presenter)
        await present(cardNavigation(screen), from: presenter)
        let scrollView = try #require(screen.scrollView)
        let cards = PublishedPocketCards(products: gameResolver(worker: workerPublishing([faceAwayLoyalty])))

        await PocketCardFaceOnOpen.apply(to: screen, for: loyaltyKey, cards: cards)

        #expect(waitUntil(on: screen) { scrollView.contentOffset.y == PocketOpenedCardView.height })
        withExtendedLifetime(window) {}
    }

    /// The card can be closed before its product answers, and a screen the
    /// user has left is not one to move.
    @Test @MainActor
    func leavesTheFaceOfACardNoLongerOnDisplay() async {
        let screen = PocketCardScreenViewController(
            card: loyaltyCard,
            product: StubSPAView(),
            surface: PocketCardSurface()
        )
        screen.view.frame = CGRect(origin: .zero, size: screenSize)
        screen.view.layoutIfNeeded()
        let cards = PublishedPocketCards(products: gameResolver(worker: workerPublishing([faceAwayLoyalty])))

        await PocketCardFaceOnOpen.apply(to: screen, for: loyaltyKey, cards: cards)
        screen.view.layoutIfNeeded()

        #expect(screen.scrollView?.contentOffset.y == 0)
    }

    /// A lookup the open gave up on must not keep running after it: a
    /// resolver that honours cancellation is told to stop.
    @Test
    func stopsTheLookupItGaveUpOn() async {
        await confirmation { lookupStopped in
            let cards = CancellationReportingCards { lookupStopped() }

            _ = await PocketCardFaceOnOpen.faceShown(for: loyaltyKey, cards: cards, timeout: .milliseconds(10))
        }
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardDefinition(
    id: PocketCardId(value: "loyalty"),
    title: "Loyalty",
    preview: .archive(path: "faces/loyalty.json")
)

private let faceAwayLoyalty = PocketCardDefinition(
    id: PocketCardId(value: "loyalty"),
    title: "Loyalty",
    preview: .archive(path: "faces/loyalty.json"),
    faceShown: false
)

private let loyaltyKey = PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty"))

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
    StubProductResolver(alwaysResolvingTo: gameProduct(worker: worker))
}

private func gameProduct(worker: ProductExecutable.Worker?) -> ResolvedProduct {
    ResolvedProduct(
        id: "game.paseo",
        displayName: "Game",
        description: nil,
        icon: nil,
        executables: ProductExecutables(app: nil, widget: nil, worker: worker),
        hasManifest: true
    )
}

/// Never answers, and reports being cancelled at the moment it is.
private struct CancellationReportingCards: PublishedPocketCardsResolving {
    let onCancel: @Sendable () -> Void

    func find(productId _: ProductId, cardId _: PocketCardId) async throws -> PublishedPocketCard {
        try await withTaskCancellationHandler {
            try await Task.sleep(for: .seconds(60))
            throw CancellationError()
        } onCancel: {
            onCancel()
        }
    }
}
