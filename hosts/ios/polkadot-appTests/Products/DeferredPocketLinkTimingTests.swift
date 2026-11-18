import Foundation
import IssueMonitoring
import Testing
@testable import polkadot_app

/// A link that arrives before the main screen registers its handlers (a cold start's) waits in
/// `DeferredLinkHandler`. The page it opens is timed from the link's arrival, not from the registration.
@MainActor
struct DeferredPocketLinkTimingTests {
    @Test
    func aLinkDeliveredBeforeRegistrationKeepsItsArrival() async throws {
        let arrivedAt = LinkArrival(date: Date(timeIntervalSince1970: 1_793_900_000.123), monotonicMilliseconds: 1_000)
        let registeredAt = LinkArrival(date: Date(timeIntervalSince1970: 1_793_900_009), monotonicMilliseconds: 9_000)
        let ingressClock = RecordingArrivalClock(arrival: arrivedAt)
        let handlerClock = RecordingArrivalClock(arrival: registeredAt)
        let (taps, continuation) = AsyncStream<HostOpenTap>.makeStream()
        let links = DeferredLinkHandler(arrivalClock: ingressClock)
        let pocket = PocketOpenService(present: { _, tap in continuation.yield(tap) }, arrivalClock: handlerClock)
        let services = URLHandlingService(children: [pocket])

        links.handle(with: URL(string: "polkadotapp://pocket/open?product=game.dot&card=loyalty")!)
        #expect(ingressClock.reads == 1)

        links.register(services)

        let tap = try await firstTap(of: taps)
        #expect(tap.source == .deeplink)
        #expect(tap.openedAtEpochMilliseconds == 1_793_900_000_123)
        #expect(tap.openedAtMonotonicMilliseconds == 1_000)
        #expect(handlerClock.reads == 0)
    }

    /// The scene reads the arrival itself, before the app starts up, and hands it over with the link.
    @Test
    func anArrivalHandedOverWithTheLinkReachesTheTapUnchanged() async throws {
        let handedOver = LinkArrival(date: Date(timeIntervalSince1970: 1_793_900_000.5), monotonicMilliseconds: 250)
        let ingressClock = RecordingArrivalClock(arrival: LinkArrival(date: Date(), monotonicMilliseconds: 7_000))
        let (taps, continuation) = AsyncStream<HostOpenTap>.makeStream()
        let links = DeferredLinkHandler(arrivalClock: ingressClock)
        let pocket = PocketOpenService(
            present: { _, tap in continuation.yield(tap) },
            arrivalClock: RecordingArrivalClock(arrival: LinkArrival(date: Date(), monotonicMilliseconds: 8_000))
        )

        let services = URLHandlingService(children: [pocket])

        links.handle(with: URL(string: "polkadotapp://pocket/open?product=game.dot&card=loyalty")!, arrival: handedOver)
        links.register(services)

        let tap = try await firstTap(of: taps)
        #expect(tap.openedAtEpochMilliseconds == 1_793_900_000_500)
        #expect(tap.openedAtMonotonicMilliseconds == 250)
        #expect(ingressClock.reads == 0)
    }
}
