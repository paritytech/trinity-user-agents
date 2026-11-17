import struct TrUAPIHost.ProductId
import Foundation
import Products
import Testing
@testable import polkadot_app

/// The navigation stack owns the opened card's screen, so closing the card frees its web view and runtime.
@MainActor
struct PocketCardHostsTests {
    @Test
    func closingTheCardFreesIt() {
        let hosts = PocketCardHosts()
        var view = hosts.open(loyalty, make: { StubExpandedPocketCardView() })
        weak var opened = view

        view = nil

        #expect(opened == nil)
        #expect(!hosts.isOnDisplay(loyalty))
    }

    /// A link to the card on screen cannot open a second screen, so its params
    /// are loaded into the page already there. The same params leave the page
    /// where the user took it.
    @Test
    func loadsNewParamsIntoThePageOnScreen() {
        let hosts = PocketCardHosts()
        let view = hosts.open(loyalty, pageQuery: "open=scratch", make: { StubExpandedPocketCardView() })
        var loaded: [String] = []

        hosts.reload(loyalty, at: "open=scratch") { _ in loaded.append("open=scratch") }
        hosts.reload(loyalty, at: "open=prizes") { _ in loaded.append("open=prizes") }
        hosts.reload(loyalty, at: "open=prizes") { _ in loaded.append("open=prizes again") }
        hosts.reload(trophy, at: "open=prizes") { _ in loaded.append("trophy") }

        #expect(loaded == ["open=prizes"])
        withExtendedLifetime(view) {}
    }
}

// MARK: - Fixtures

private let loyalty = PocketCardKey(productId: try! ProductId("game.paseo"), cardId: PocketCardId(value: "loyalty"))
private let trophy = PocketCardKey(productId: try! ProductId("game.paseo"), cardId: PocketCardId(value: "trophy"))
