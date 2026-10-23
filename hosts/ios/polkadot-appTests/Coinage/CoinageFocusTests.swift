import Coinage
import CoreGraphics
import Foundation
import Testing

@testable import polkadot_app

/// Picking a coin out: what it says about itself, where it goes, and how a tap finds it.
@Suite("Coin trace")
struct CoinageTraceTests {
    private static let installation = try! CoinageInstallationId(value: Data(repeating: 3, count: 32))

    private func keyIndex(_ item: Int) -> CoinageKeyIndex {
        CoinageKeyIndex(installation: Self.installation, item: DerivationIndex(item))
    }

    private func coin(_ item: Int, exponent: Int16 = 4, fungibility: UInt8?, hops: [Hop]) -> Coin {
        Coin(
            exponent: exponent,
            derivationIndex: keyIndex(item),
            age: 0,
            recyclerFungibility: fungibility,
            hops: hops,
            publicKey: Data(repeating: UInt8(item), count: 32)
        )
    }

    private func voucher(_ item: Int, fungibility: UInt8) -> Voucher {
        Voucher(
            exponent: 6,
            derivationIndex: keyIndex(item),
            allocatedAt: .distantPast,
            readyAt: .distantPast,
            remoteState: .inRecycler(.init(index: 0, membersCount: 10)),
            recyclerFungibility: fungibility,
            maxRecyclerFungibility: 100,
            publicKey: Data(repeating: 9, count: 32)
        )
    }

    private func traces(_ holdings: CoinageHoldings) -> [String: CoinageCoinTrace] {
        CoinageBreakdownFactory.traces(from: holdings) { "2^\($0)" }
    }

    @Test("A coin that cleared here is told its hops and the grade its clearing earned")
    func clearedCoinIsGraded() throws {
        let holdings = CoinageHoldings(
            coins: [.init(
                coin: coin(1, fungibility: 100, hops: [.split(fanout: 4), .transfer(bundleSize: 3)]),
                availability: .availableNow
            )],
            vouchers: []
        )

        let trace = try #require(traces(holdings)["coin-\(keyIndex(1))"])

        #expect(trace.value == "2^4")
        #expect(trace.partition == .ready)
        #expect(trace.hops == 2)
        #expect(trace.quality == .high)
    }

    @Test("A coin received from someone else has no clearing to grade")
    func receivedCoinIsUnknown() throws {
        let holdings = CoinageHoldings(
            coins: [.init(
                coin: coin(2, fungibility: nil, hops: [.transfer(bundleSize: 1), .transfer(bundleSize: 2)]),
                availability: .gainingPrivacy
            )],
            vouchers: []
        )

        let trace = try #require(traces(holdings)["coin-\(keyIndex(2))"])

        #expect(trace.partition == .clearing)
        #expect(trace.hops == 2)
        #expect(trace.quality == .unknown)
    }

    @Test("A holding still clearing has made no hops and is graded as it stands")
    func clearingHoldingHasNoHops() throws {
        let holdings = CoinageHoldings(
            coins: [],
            vouchers: [.init(voucher: voucher(3, fungibility: 0), availability: .pending)]
        )

        let trace = try #require(traces(holdings)["voucher-\(keyIndex(3))"])

        #expect(trace.value == "2^6")
        #expect(trace.hops == 0)
        #expect(trace.quality == .veryLow)
    }

    @Test("The grades double in width up the score, and high is the top band from sixty-five")
    func gradesDoubleUpTheScore() {
        #expect(CoinageCoinTrace.Quality(score: 0) == .veryLow)
        #expect(CoinageCoinTrace.Quality(score: 8) == .veryLow)
        #expect(CoinageCoinTrace.Quality(score: 9) == .low)
        #expect(CoinageCoinTrace.Quality(score: 16) == .low)
        #expect(CoinageCoinTrace.Quality(score: 17) == .fair)
        #expect(CoinageCoinTrace.Quality(score: 32) == .fair)
        #expect(CoinageCoinTrace.Quality(score: 33) == .good)
        #expect(CoinageCoinTrace.Quality(score: 64) == .good)
        #expect(CoinageCoinTrace.Quality(score: 65) == .high)
        #expect(CoinageCoinTrace.Quality(score: 100) == .high)
    }

    @Test("No score at all is unknown, never a grade")
    func noScoreIsUnknown() {
        #expect(CoinageBreakdownFactory.quality(forScore: nil) == .unknown)
        #expect(CoinageBreakdownFactory.quality(forScore: 0) == .veryLow)
    }

    @Test("The traces are keyed the way the coins are, so a tap finds its own")
    func tracesMatchTheCoins() {
        let holdings = CoinageHoldings(
            coins: [.init(coin: coin(4, fungibility: 80, hops: []), availability: .availableNow)],
            vouchers: [.init(voucher: voucher(5, fungibility: 20), availability: .pending)]
        )

        let ids = Set(CoinageBreakdownFactory.stripCoins(CoinageBreakdownFactory.rows(from: holdings)).map(\.id))

        #expect(Set(traces(holdings).keys) == ids)
    }

    @Test("The band is never shorter than the coin, whatever the trace")
    func bandFitsTheCoin() {
        let trace = CoinageCoinTrace(value: "", partition: .ready, hops: 3, quality: .fair)

        #expect(CoinageTraceView.height(for: trace) >= CoinageArrangement.focusBandMinimum)
    }
}

@Suite("Picked coin")
struct CoinageFocusTests {
    /// None: the grid never reads them and the strip falls back to the base thickness without.
    private let designs: [CoinageAssetStore.Design] = []

    private func coins(_ count: Int) -> [CoinageScene.Coin] {
        (0 ..< count).map {
            CoinageScene.Coin(id: "c\($0)", exponent: Int16($0 % 15), wear: 0.5, partition: .ready, level: 4, hops: 0)
        }
    }

    private func targets(
        _ coins: [CoinageScene.Coin],
        arrangement: CoinageArrangement,
        focus: CoinageArrangement.Focus?
    ) -> CoinageArrangement.Result {
        CoinageArrangement.targets(
            for: coins,
            arrangement: arrangement,
            focus: focus,
            area: CGSize(width: 340, height: 500),
            designs: designs,
            stripHeight: 60
        )
    }

    @Test("The picked coin goes large into a band at the foot of the screen, over the rest, which fade in place")
    func pickedCoinTakesTheBand() throws {
        // Three hundred coins pack to the smallest diameter and run to over a thousand points.
        let coins = coins(300)
        let plain = targets(coins, arrangement: .grid, focus: nil)
        var focus = CoinageArrangement.Focus(id: "c2", band: 150)
        focus.window = 200
        focus.viewport = 600
        let picked = targets(coins, arrangement: .grid, focus: focus)

        // A tall grid: the band floats at the bottom of what is on screen and the field keeps
        // its height.
        #expect(plain.height > 800)
        #expect(picked.height == plain.height)
        #expect(picked.focusTop == 650 - CoinageArrangement.focusInset)

        let chosen = try #require(picked.targets.first { $0.coin.id == "c2" }?.target)

        #expect(chosen.height == CoinageArrangement.focusDiameter)
        // Centred on the coin's own band, whatever the trace's height asks for beside it.
        #expect(chosen.centre.y == 650 - CoinageArrangement.focusInset + CoinageArrangement.focusBandMinimum / 2)
        #expect(chosen.turn == 0)
        #expect(chosen.recede == 0)
        #expect(chosen.lift == CoinageArrangement.focusLift)

        for (before, after) in zip(plain.targets, picked.targets) where before.coin.id != "c2" {
            #expect(after.target.recede == 1)
            #expect(after.target.centre == before.target.centre)
        }

        #expect(picked.blocks == plain.blocks)
    }

    @Test("A short grid gets the band below its coins rather than over them")
    func shortGridKeepsItsCoinsUncovered() throws {
        let coins = coins(6)
        let plain = targets(coins, arrangement: .grid, focus: nil)
        var focus = CoinageArrangement.Focus(id: "c2", band: 150)
        focus.viewport = 600
        let picked = targets(coins, arrangement: .grid, focus: focus)

        #expect(picked.focusTop == plain.height)
        #expect(picked.height == plain.height + 150)
    }

    @Test("A band shorter than the coin is made tall enough for it")
    func bandIsNeverShorterThanTheCoin() throws {
        let coins = coins(3)
        let picked = targets(coins, arrangement: .strip, focus: .init(id: "c0", band: 10))
        let chosen = try #require(picked.targets.first { $0.coin.id == "c0" }?.target)

        #expect(chosen.centre.y == 60 + CoinageArrangement.focusBandMinimum / 2)
        #expect(picked.height == 60 + CoinageArrangement.focusBandMinimum)
    }

    @Test("Picking a coin that is not there changes nothing")
    func missingCoinIsIgnored() {
        let coins = coins(3)
        let plain = targets(coins, arrangement: .grid, focus: nil)
        let picked = targets(coins, arrangement: .grid, focus: .init(id: "gone", band: 150))

        #expect(picked.height == plain.height)
        #expect(picked.targets.map(\.target) == plain.targets.map(\.target))
    }

    @Test("A tap lands on the coin under it, and nowhere between coins")
    func tapFindsTheCoinUnderIt() throws {
        let coins = coins(4)
        let field = CoinageCoinField()
        let laid = targets(coins, arrangement: .grid, focus: nil)
        field.retarget(laid.targets, spawningFrom: 340, stagger: .fromFront)

        // Long enough for every spring to settle where it was aimed.
        for _ in 0 ..< 600 {
            field.advance(by: 1.0 / 60)
        }

        let second = try #require(laid.targets.first { $0.coin.id == "c1" }?.target)

        #expect(CoinageScene.coin(at: second.centre, in: field) == "c1")
        #expect(CoinageScene.coin(at: CGPoint(x: 339, y: 499), in: field) == nil)
    }

    @Test("An edge-on coin can still be tapped within a finger's width of its edge")
    func edgeOnCoinHasAFloor() throws {
        let coins = coins(1)
        let field = CoinageCoinField()
        var laid = targets(coins, arrangement: .strip, focus: nil)
        var target = laid.targets[0].target
        target.turn = .pi / 2
        laid = CoinageArrangement.Result(targets: [(coin: coins[0], target: target)], height: 60, blocks: [], runs: [])
        field.retarget(laid.targets, spawningFrom: 340, stagger: .fromFront)

        for _ in 0 ..< 600 {
            field.advance(by: 1.0 / 60)
        }

        let beside = CGPoint(x: target.centre.x + CoinageScene.touchFloor / 2 - 1, y: target.centre.y)
        let clear = CGPoint(x: target.centre.x + CoinageScene.touchFloor, y: target.centre.y)

        #expect(CoinageScene.coin(at: beside, in: field) == "c0")
        #expect(CoinageScene.coin(at: clear, in: field) == nil)
    }
}
