import Foundation
import IssueMonitoring
import Testing
@testable import polkadot_app

/// The app's own steps, compared whole against literals with a fixed origin, session and clock.
struct HostLoadEventTests {
    private typealias F = HostEventFixtures

    @Test func rpcConnectedNamesTheChainAndTheExecutableItServes() {
        let event = HostLoadEvent.rpcConnected(
            chain: "asset_hub",
            executable: HostExecutableOwner(productID: "peopl.testnet", kind: .worker, hostSessionID: F.session),
            outcome: .ok,
            durationMilliseconds: 436,
            origin: F.origin,
            monotonicMilliseconds: 6_916
        )

        #expect(event == HostEvent(
            name: "host.rpc.connected",
            tags: F.stepTags(outcome: "ok", session: true).merging([
                "rpc.chain": "asset_hub",
                "product.id": "peopl.testnet",
                "executable.kind": "worker"
            ]) { _, new in new },
            extras: F.stepExtras(duration: 436, monotonic: 6_916, session: true),
            durationMilliseconds: 436
        ))
    }

    @Test func aCoreConnectionNamesNoProduct() {
        let event = HostLoadEvent.rpcConnected(
            chain: "people",
            executable: nil,
            outcome: .error,
            durationMilliseconds: 12,
            origin: F.origin,
            monotonicMilliseconds: 7_000
        )

        #expect(event == HostEvent(
            name: "host.rpc.connected",
            tags: F.stepTags(outcome: "error", session: false).merging(["rpc.chain": "people"]) { _, new in new },
            extras: F.stepExtras(duration: 12, monotonic: 7_000, session: false),
            durationMilliseconds: 12
        ))
    }

    @Test func faceLoadedSaysWhetherItsWorkerWasRunning() {
        let event = HostLoadEvent.pocketFaceLoaded(
            productID: "peopl.testnet",
            outcome: .noWorker,
            worker: .cold,
            durationMilliseconds: 24,
            hostSessionID: nil,
            origin: F.origin,
            monotonicMilliseconds: 24
        )

        #expect(event == HostEvent(
            name: "host.pocket.face_loaded",
            tags: F.stepTags(outcome: "no_worker", session: false).merging([
                "product.id": "peopl.testnet",
                "worker.state": "cold"
            ]) { _, new in new },
            extras: F.stepExtras(duration: 24, monotonic: 24, session: false),
            durationMilliseconds: 24
        ))
    }

    @Test func workerBootReportsTheTimedPhasesOnly() {
        let event = HostLoadEvent.workerBoot(
            productID: "peopl.testnet",
            outcome: .ok,
            phases: HostWorkerBootPhases(resolve: 120, executionOpen: 8, webViewCreate: 2),
            durationMilliseconds: 900,
            hostSessionID: F.session,
            origin: F.origin,
            monotonicMilliseconds: 1_000
        )

        #expect(event == HostEvent(
            name: "host.worker.boot",
            tags: F.stepTags(outcome: "ok", session: true).merging(["product.id": "peopl.testnet"]) { _, new in new },
            extras: F.stepExtras(duration: 900, monotonic: 1_000, session: true).merging([
                "resolve_ms": .integer(120),
                "execution_open_ms": .integer(8),
                "webview_create_ms": .integer(2)
            ]) { _, new in new },
            durationMilliseconds: 900
        ))
    }

    @Test func runtimeBootIsAStepWithNoSession() {
        let event = HostLoadEvent.runtimeBoot(
            outcome: .error,
            durationMilliseconds: 31,
            origin: F.origin,
            monotonicMilliseconds: 31
        )

        #expect(event == HostEvent(
            name: "host.runtime.boot",
            tags: F.stepTags(outcome: "error", session: false),
            extras: F.stepExtras(duration: 31, monotonic: 31, session: false),
            durationMilliseconds: 31
        ))
    }

    @Test func pageLoadedCarriesTheTimeFromTheTap() {
        let event = HostLoadEvent.webViewPageLoaded(
            hostSessionID: F.session,
            productID: "peopl.testnet",
            outcome: .abandoned,
            durationMilliseconds: 6_349,
            tapMilliseconds: 6_900,
            origin: F.origin,
            monotonicMilliseconds: 1_830_576
        )

        #expect(event == HostEvent(
            name: "host.webview.page_loaded",
            tags: F.stepTags(outcome: "abandoned", session: true)
                .merging(["product.id": "peopl.testnet"]) { _, new in new },
            extras: F.stepExtras(duration: 6_349, monotonic: 1_830_576, session: true).merging([
                "tap_ms": .integer(6_900)
            ]) { _, new in new },
            durationMilliseconds: 6_349
        ))
    }

    @Test func aReusedPageCarriesTheReuseTimeTwice() {
        let event = HostLoadEvent.pocketCardTapped(
            productID: "peopl.testnet",
            outcome: .reused,
            source: .pocketCard,
            durationMilliseconds: 180,
            hostSessionID: F.session,
            origin: F.origin,
            monotonicMilliseconds: 9
        )

        #expect(event == HostEvent(
            name: "host.pocket.card_tapped",
            tags: F.stepTags(outcome: "reused", session: true).merging([
                "product.id": "peopl.testnet",
                "open.source": "pocket_card"
            ]) { _, new in new },
            extras: F.stepExtras(duration: 180, monotonic: 9, session: true).merging([
                "reuse_ms": .integer(180)
            ]) { _, new in new },
            durationMilliseconds: 180
        ))
    }

    @Test func anOpenedPageIsTimedToItsLoadStarting() {
        let event = HostLoadEvent.pocketCardTapped(
            productID: "peopl.testnet",
            outcome: .opened,
            source: .deeplink,
            durationMilliseconds: 35,
            hostSessionID: F.session,
            origin: F.origin,
            monotonicMilliseconds: 9
        )

        #expect(event == HostEvent(
            name: "host.pocket.card_tapped",
            tags: F.stepTags(outcome: "opened", session: true).merging([
                "product.id": "peopl.testnet",
                "open.source": "deeplink"
            ]) { _, new in new },
            extras: F.stepExtras(duration: 35, monotonic: 9, session: true),
            durationMilliseconds: 35
        ))
    }
}
