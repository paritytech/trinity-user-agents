import Foundation
import Testing
@testable import polkadot_app

struct PocketOpenServiceTests {
    @Test
    func claimsAPocketLinkAndPassesItOn() async throws {
        let seen = Recorder()
        let service = PocketOpenService(present: { seen.record($0) })

        #expect(service.handle(url: URL(string: "polkadot://game.dot/-/pocket/add?card=loyalty")!))

        try await Task.sleep(for: .milliseconds(50))
        #expect(seen.links.map(\.cardId.value) == ["loyalty"])
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
        let service = PocketOpenService(present: { seen.record($0) }, refuse: { seen.refusals.append($0) })
        let hidden = "polkadot://game.dot/-/pocket/add?card=loy%E2%80%8Balty"

        #expect(service.handle(url: URL(string: hidden)!))

        try await Task.sleep(for: .milliseconds(50))
        #expect(seen.links.isEmpty)
        #expect(seen.refusals.count == 1)
    }

    /// The core sends an action it does not serve to the App on purpose, so a
    /// link minted for a newer host degrades instead of failing. Claiming it
    /// here would turn every future Pocket verb into a refusal on this build.
    @Test
    func doesNotClaimAPocketActionTheCoreDoesNotServe() async throws {
        let seen = Recorder()
        let service = PocketOpenService(present: { seen.record($0) }, refuse: { seen.refusals.append($0) })

        #expect(!service.handle(url: URL(string: "polkadot://game.dot/-/pocket/pin?card=loyalty")!))

        try await Task.sleep(for: .milliseconds(50))
        #expect(seen.links.isEmpty)
        #expect(seen.refusals.isEmpty)
    }

    /// A link under the reserved target that names no card at all is still the
    /// host's to answer.
    @Test
    func claimsALinkThatNamesNoCard() async throws {
        let seen = Recorder()
        let service = PocketOpenService(present: { seen.record($0) }, refuse: { seen.refusals.append($0) })

        #expect(service.handle(url: URL(string: "polkadot://game.dot/-/pocket/add")!))

        try await Task.sleep(for: .milliseconds(50))
        #expect(seen.links.isEmpty)
        #expect(seen.refusals.count == 1)
    }
}

private final class Recorder: @unchecked Sendable {
    private(set) var links: [PocketDeeplink] = []
    var refusals: [String] = []

    func record(_ link: PocketDeeplink) {
        links.append(link)
    }
}
