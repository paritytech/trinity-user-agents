import Foundation
import StructuredConcurrency
import Testing
@testable import polkadot_app

struct PocketOpenServiceTests {
    @Test
    func claimsAPocketLinkAndPassesItOn() async throws {
        let seen = Recorder()
        let service = seen.makeService()

        #expect(service.handle(url: URL(string: "polkadot://game.dot/-/pocket/add?card=loyalty")!))

        #expect(try await seen.answer() == .presented(cardId: "loyalty"))
    }

    /// A bare `/-/pocket` is the host's to answer: it opens the Pocket, with no
    /// card to present and nothing to refuse.
    @Test
    func claimsACollectionLinkAndOpensThePocket() async throws {
        let seen = Recorder()
        let service = seen.makeService()

        #expect(service.handle(url: URL(string: "polkadot://game.dot/-/pocket")!))

        #expect(try await seen.answer() == .openedCollection)
    }

    /// An ordinary product link is not ours and must fall through to the App
    /// handler behind us in the chain.
    @Test
    func doesNotClaimAnOrdinaryProductLink() {
        let service = PocketOpenService(present: { _ in })

        #expect(!service.handle(url: URL(string: "polkadot://game.dot/some/page")!))
    }

    /// The reserved target belongs to the host, so a card id the core would
    /// refuse is answered here, with a message, rather than opening the
    /// product's own page.
    @Test
    func claimsAndRefusesAHiddenCardId() async throws {
        let seen = Recorder()
        let service = seen.makeService()
        let hidden = "polkadot://game.dot/-/pocket/add?card=loy%E2%80%8Balty"

        #expect(service.handle(url: URL(string: hidden)!))

        #expect(try await seen.answer() == .refused)
    }

    /// The core sends an action it does not serve to the App on purpose, so a
    /// link minted for a newer host degrades instead of failing. Claiming it
    /// here would turn every future Pocket verb into a refusal on this build.
    @Test
    func doesNotClaimAPocketActionTheCoreDoesNotServe() {
        let service = PocketOpenService(present: { _ in })

        #expect(!service.handle(url: URL(string: "polkadot://game.dot/-/pocket/pin?card=loyalty")!))
    }

    /// A link under the reserved target that names no card at all is still the
    /// host's to answer.
    @Test
    func claimsALinkThatNamesNoCard() async throws {
        let seen = Recorder()
        let service = seen.makeService()

        #expect(service.handle(url: URL(string: "polkadot://game.dot/-/pocket/add")!))

        #expect(try await seen.answer() == .refused)
    }
}

private final class Recorder: @unchecked Sendable {
    enum Event: Equatable, Sendable { case presented(cardId: String), openedCollection, refused }
    private let events: AsyncStream<Event>
    private let continuation: AsyncStream<Event>.Continuation

    init() {
        (events, continuation) = AsyncStream.makeStream()
    }

    /// Every callback is wired, so a wrong answer fails naming it rather than timing out.
    func makeService() -> PocketOpenService {
        PocketOpenService(
            present: { self.record(.presented(cardId: $0.cardId.value)) },
            openCollection: { self.record(.openedCollection) },
            refuse: { _ in self.record(.refused) }
        )
    }

    private func record(_ event: Event) {
        continuation.yield(event)
    }

    /// The one answer `handle` dispatched; fails rather than hangs if none arrives.
    func answer() async throws -> Event {
        try await withTimeout(.seconds(5)) { [events] in
            for await event in events {
                return event
            }
            throw CancellationError()
        }
    }
}
