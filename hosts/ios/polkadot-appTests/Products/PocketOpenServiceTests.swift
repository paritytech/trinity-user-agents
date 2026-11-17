import Foundation
import IssueMonitoring
import StructuredConcurrency
import StructuredConcurrencyTestSupport
import Testing
@testable import polkadot_app

struct PocketOpenServiceTests {
    @Test
    func claimsAPocketLinkAndPassesItOn() async throws {
        let seen = Recorder()
        let service = seen.makeService()

        #expect(service.handle(url: URL(string: "polkadotapp://pocket/add?product=game.dot&card=loyalty")!))

        #expect(try await seen.answer() == .presented(cardId: "loyalty"))
    }

    /// An open link is the user's tap for its page. Handed straight here, its arrival is read inside `handle`,
    /// before the hops onto the main actor that present it. The test holds the main actor from `handle` to the
    /// read count, so a reading taken in the dispatched task would count 0 there.
    @MainActor
    @Test
    func anOpenLinkIsTimedFromWhenItArrives() async throws {
        let (taps, continuation) = AsyncStream<HostOpenTap>.makeStream()
        let arrival = LinkArrival(date: Date(timeIntervalSince1970: 1_793_900_000.123), monotonicMilliseconds: 4_200)
        let clock = RecordingArrivalClock(arrival: arrival)
        let service = PocketOpenService(present: { _, tap in continuation.yield(tap) }, arrivalClock: clock)

        #expect(service.handle(url: URL(string: "polkadotapp://pocket/open?product=game.dot&card=loyalty")!))
        #expect(clock.reads == 1)

        let tap = try await firstTap(of: taps)
        #expect(tap.source == .deeplink)
        #expect(tap.openedAtEpochMilliseconds == 1_793_900_000_123)
        #expect(tap.openedAtMonotonicMilliseconds == 4_200)
        #expect(clock.reads == 1)
    }

    /// A bare `polkadotapp://pocket` is the host's to answer: it opens the Pocket,
    /// with no card to present and nothing to refuse.
    @Test
    func claimsACollectionLinkAndOpensThePocket() async throws {
        let seen = Recorder()
        let service = seen.makeService()

        #expect(service.handle(url: URL(string: "polkadotapp://pocket")!))

        #expect(try await seen.answer() == .openedCollection)
    }

    /// An ordinary product link is not ours and must fall through to the App
    /// handler behind us in the chain.
    @Test
    func doesNotClaimAnOrdinaryProductLink() {
        let service = PocketOpenService(present: { _, _ in })

        #expect(!service.handle(url: URL(string: "polkadot://game.dot/some/page")!))
    }

    /// The Pocket handler sits ahead of other app links in the chain, such as a
    /// cheque, which must still reach their own handlers.
    @Test
    func doesNotClaimAnotherAppLink() {
        let service = PocketOpenService(present: { _, _ in })
        let cheque = "\(DeeplinkTestSchemes.active)://pay/cheque?id=abc"

        #expect(!service.handle(url: URL(string: cheque)!))
    }

    /// A Pocket link belongs to the host, so a card id the core would refuse is
    /// answered here, with a message, rather than dropped.
    @Test
    func claimsAndRefusesAHiddenCardId() async throws {
        let seen = Recorder()
        let service = seen.makeService()
        let hidden = "polkadotapp://pocket/add?product=game.dot&card=loy%E2%80%8Balty"

        #expect(service.handle(url: URL(string: hidden)!))

        #expect(try await seen.answer() == .refused)
    }

    /// A Pocket action this build does not serve has nowhere else to go, so the
    /// user is told rather than left with a link that does nothing.
    @Test
    func claimsAndRefusesAPocketActionTheCoreDoesNotServe() async throws {
        let seen = Recorder()
        let service = seen.makeService()

        #expect(service.handle(url: URL(string: "polkadotapp://pocket/pin?product=game.dot&card=loyalty")!))

        #expect(try await seen.answer() == .refused)
    }

    /// A Pocket link that names no card at all is still the host's to answer.
    @Test
    func claimsALinkThatNamesNoCard() async throws {
        let seen = Recorder()
        let service = seen.makeService()

        #expect(service.handle(url: URL(string: "polkadotapp://pocket/add?product=game.dot")!))

        #expect(try await seen.answer() == .refused)
    }
}

/// Hands out one arrival and counts how often it was read.
final class RecordingArrivalClock: LinkArrivalClock, @unchecked Sendable {
    private let arrival: LinkArrival
    private let lock = NSLock()
    private var readCount = 0

    init(arrival: LinkArrival) {
        self.arrival = arrival
    }

    var reads: Int {
        lock.withLock { readCount }
    }

    func read() -> LinkArrival {
        lock.withLock { readCount += 1 }
        return arrival
    }
}

/// The first tap a service presents; fails rather than hangs if none arrives.
func firstTap(of taps: AsyncStream<HostOpenTap>) async throws -> HostOpenTap {
    try await withTimeout(.hangGuard) {
        for await tap in taps {
            return tap
        }
        throw CancellationError()
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
            present: { link, _ in self.record(.presented(cardId: link.cardId.value)) },
            openCollection: { self.record(.openedCollection) },
            refuse: { _ in self.record(.refused) }
        )
    }

    private func record(_ event: Event) {
        continuation.yield(event)
    }

    /// The one answer `handle` dispatched; fails rather than hangs if none arrives.
    func answer() async throws -> Event {
        try await withTimeout(.hangGuard) { [events] in
            for await event in events {
                return event
            }
            throw CancellationError()
        }
    }
}
