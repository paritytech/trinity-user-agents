import Foundation
import Testing
import IssueMonitoring

struct HostEventTests {
    @Test func processStartedKeepsNumericMeasurementsTyped() throws {
        let origin = try HostEventOrigin(
            launchID: #require(UUID(uuidString: "18a5320e-3a4d-46df-ad7a-ebba4ef3ee3f")),
            osVersion: "26.5.1",
            appVersion: "1.2.3",
            appBuild: 456,
            appVariant: "nightly"
        )

        let event = HostEvent.processStarted(
            origin: origin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 42
        )

        #expect(event.name == "host.process.started")
        #expect(event.tags["host.app_variant"] == "nightly")
        #expect(event.tags["host.os_version"] == "26.5.1")
        #expect(event.tags["host.native_app_version"] == "1.2.3")
        #expect(event.tags["truapi.host_version"] == "0.9.0")
        #expect(event.extras["app.launch_id"] == .string("18a5320e-3a4d-46df-ad7a-ebba4ef3ee3f"))
        #expect(event.extras["host.app_build"] == .integer(456))
        #expect(event.extras["event.monotonic_ms"] == .integer(42))
    }

    @Test func webViewCreatedCarriesItsCorrelationIDAsATagAndAnExtra() throws {
        let origin = HostEventOrigin(
            launchID: UUID(),
            osVersion: "26.5",
            appVersion: nil,
            appBuild: nil,
            appVariant: "debug"
        )
        let hostSessionID = try #require(UUID(uuidString: "d93fe846-8f3c-4cfa-a440-402b6f7b43d3"))

        let event = HostEvent.webViewCreated(
            hostSessionID: hostSessionID,
            origin: origin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 84
        )

        #expect(event.name == "host.webview.created")
        #expect(event.tags["host.session_id"] == "d93fe846-8f3c-4cfa-a440-402b6f7b43d3")
        #expect(event.extras["host.session_id"] == .string("d93fe846-8f3c-4cfa-a440-402b6f7b43d3"))
        #expect(event.extras["event.monotonic_ms"] == .integer(84))
        #expect(event.extras["app.launch_id"] == nil)
        #expect(event.tags["host.os_version"] == nil)
        #expect(event.durationMilliseconds == nil)
    }

    /// Sentry can search and chart tags, not extras, so the join key is a tag.
    @Test func productResolvedCarriesItsOutcomeTriggerDurationAndSession() throws {
        let hostSessionID = try #require(UUID(uuidString: "d93fe846-8f3c-4cfa-a440-402b6f7b43d3"))

        let event = HostEvent.productResolved(
            productID: "peopl.testnet",
            outcome: .ok,
            trigger: .open,
            durationMilliseconds: 555,
            hostSessionID: hostSessionID,
            origin: Self.origin,
            monotonicMilliseconds: 2_092
        )

        #expect(event.name == "host.product.resolved")
        #expect(event.tags["product.id"] == "peopl.testnet")
        #expect(event.tags["outcome"] == "ok")
        #expect(event.tags["trigger"] == "open")
        #expect(event.tags["host.platform"] == "ios")
        #expect(event.tags["host.app_variant"] == "nightly")
        #expect(event.tags["host.session_id"] == "d93fe846-8f3c-4cfa-a440-402b6f7b43d3")
        #expect(event.extras["duration_ms"] == .integer(555))
        #expect(event.durationMilliseconds == 555)
        #expect(event.extras["event.monotonic_ms"] == .integer(2_092))
        #expect(event.extras["host.app_build"] == .integer(456))
        #expect(event.extras["host.session_id"] == .string("d93fe846-8f3c-4cfa-a440-402b6f7b43d3"))
    }

    @Test func aStepOutsideAnySessionCarriesNoSessionID() {
        let event = HostEvent.productResolved(
            productID: "peopl.testnet",
            outcome: .cacheHit,
            trigger: .prewarm,
            durationMilliseconds: 12,
            hostSessionID: nil,
            origin: Self.origin,
            monotonicMilliseconds: 40
        )

        #expect(event.tags["outcome"] == "cache_hit")
        #expect(event.tags["trigger"] == "prewarm")
        #expect(event.tags["host.session_id"] == nil)
        #expect(event.extras["host.session_id"] == nil)
    }

    @Test func bulletinFetchKeepsItsByteCountAndContentHashReadNumeric() {
        let downloaded = HostEvent.bulletinFetch(
            domain: "app.peopl.testnet",
            outcome: .ok,
            trigger: .refresh,
            durationMilliseconds: 3_803,
            contentHashMilliseconds: 268,
            bytes: 9_497_026,
            hostSessionID: nil,
            origin: Self.origin,
            monotonicMilliseconds: 6_170
        )

        #expect(downloaded.name == "host.bulletin.fetch")
        #expect(downloaded.extras["dotns.domain"] == .string("app.peopl.testnet"))
        #expect(downloaded.tags["dotns.domain"] == nil)
        #expect(downloaded.tags["outcome"] == "ok")
        #expect(downloaded.tags["trigger"] == "refresh")
        #expect(downloaded.extras["duration_ms"] == .integer(3_803))
        #expect(downloaded.extras["dotns.content_hash_ms"] == .integer(268))
        #expect(downloaded.extras["bulletin.bytes"] == .integer(9_497_026))

        let applied = HostEvent.bulletinFetch(
            domain: "app.peopl.testnet",
            outcome: .cacheHit,
            trigger: .open,
            durationMilliseconds: 1,
            contentHashMilliseconds: nil,
            bytes: nil,
            hostSessionID: nil,
            origin: Self.origin,
            monotonicMilliseconds: 683
        )

        #expect(applied.tags["outcome"] == "cache_hit")
        #expect(applied.extras["bulletin.bytes"] == nil)
        #expect(applied.extras["dotns.content_hash_ms"] == nil)
    }

    @Test func rpcConnectedNamesTheExecutableItServes() throws {
        let hostSessionID = try #require(UUID(uuidString: "6564F4CC-E69C-4052-BD4A-433F66790125"))

        let worker = HostEvent.rpcConnected(
            chain: "people",
            executable: HostExecutableOwner(productID: "peopl.testnet", kind: .worker, hostSessionID: hostSessionID),
            outcome: .ok,
            durationMilliseconds: 436,
            origin: Self.origin,
            monotonicMilliseconds: 6_916
        )
        let core = HostEvent.rpcConnected(
            chain: "people",
            executable: nil,
            outcome: .error,
            durationMilliseconds: 12,
            origin: Self.origin,
            monotonicMilliseconds: 7_000
        )

        #expect(worker.name == "host.rpc.connected")
        #expect(worker.tags["chain"] == "people")
        #expect(worker.tags["product.id"] == "peopl.testnet")
        #expect(worker.tags["executable.kind"] == "worker")
        #expect(worker.tags["host.session_id"] == "6564f4cc-e69c-4052-bd4a-433f66790125")
        #expect(worker.extras["duration_ms"] == .integer(436))
        #expect(core.tags["product.id"] == nil)
        #expect(core.tags["executable.kind"] == nil)
        #expect(core.tags["host.session_id"] == nil)
    }

    @Test func faceLoadedSaysWhetherItsWorkerWasRunning() {
        let face = HostEvent.pocketFaceLoaded(
            productID: "peopl.testnet",
            outcome: .noWorker,
            worker: .cold,
            durationMilliseconds: 24,
            hostSessionID: nil,
            origin: Self.origin,
            monotonicMilliseconds: 24
        )

        #expect(face.name == "host.pocket.face_loaded")
        #expect(face.tags["product.id"] == "peopl.testnet")
        #expect(face.tags["outcome"] == "no_worker")
        #expect(face.tags["worker"] == "cold")
        #expect(face.tags["host.session_id"] == nil)
    }

    @Test func workerBootReportsTheTimedPhasesOnly() throws {
        let hostSessionID = try #require(UUID(uuidString: "6564F4CC-E69C-4052-BD4A-433F66790125"))

        let boot = HostEvent.workerBoot(
            productID: "peopl.testnet",
            outcome: .ok,
            phases: HostWorkerBootPhases(resolve: 120, executionOpen: 8, webViewCreate: 2),
            durationMilliseconds: 900,
            hostSessionID: hostSessionID,
            origin: Self.origin,
            monotonicMilliseconds: 1_000
        )

        #expect(boot.name == "host.worker.boot")
        #expect(boot.tags["outcome"] == "ok")
        #expect(boot.tags["host.session_id"] == "6564f4cc-e69c-4052-bd4a-433f66790125")
        #expect(boot.extras["resolve_ms"] == .integer(120))
        #expect(boot.extras["execution_open_ms"] == .integer(8))
        #expect(boot.extras["webview_create_ms"] == .integer(2))
        #expect(boot.extras["blank_ready_ms"] == nil)
        #expect(boot.extras["duration_ms"] == .integer(900))
    }

    @Test func runtimeBootIsAStepWithNoSession() {
        let boot = HostEvent.runtimeBoot(
            outcome: .error,
            durationMilliseconds: 31,
            origin: Self.origin,
            monotonicMilliseconds: 31
        )

        #expect(boot.name == "host.runtime.boot")
        #expect(boot.tags["outcome"] == "error")
        #expect(boot.durationMilliseconds == 31)
        #expect(boot.tags["host.session_id"] == nil)
    }

    @Test func pageLoadedCarriesTheSessionItsPageReads() throws {
        let hostSessionID = try #require(UUID(uuidString: "6564F4CC-E69C-4052-BD4A-433F66790125"))

        let event = HostEvent.webViewPageLoaded(
            hostSessionID: hostSessionID,
            productID: "peopl.testnet",
            outcome: .abandoned,
            durationMilliseconds: 6_349,
            origin: Self.origin,
            monotonicMilliseconds: 1_830_576
        )

        #expect(event.name == "host.webview.page_loaded")
        #expect(event.tags["product.id"] == "peopl.testnet")
        #expect(event.tags["outcome"] == "abandoned")
        #expect(event.extras["duration_ms"] == .integer(6_349))
        #expect(event.extras["event.monotonic_ms"] == .integer(1_830_576))
        #expect(event.tags["host.session_id"] == "6564f4cc-e69c-4052-bd4a-433f66790125")
    }

    @Test func aDisabledLoadTelemetryNeverBuildsTheEvent() {
        var built = false

        HostLoadTelemetry.disabled.record { origin, now in
            built = true
            return .runtimeBoot(
                outcome: .ok,
                durationMilliseconds: 0,
                origin: origin,
                monotonicMilliseconds: now
            )
        }

        #expect(!built)
    }

    @Test func aDisabledLoadTelemetryReadsNoClockAndBindsNoContext() async {
        let telemetry = HostLoadTelemetry.disabled

        let context = await telemetry.withContext(session: UUID(), trigger: .open) { telemetry.context }

        #expect(!telemetry.isEnabled)
        #expect(telemetry.now() == nil)
        #expect(telemetry.makeSessionID() == nil)
        #expect(context == nil)
    }

    @Test func loadTelemetryStampsTheEventWithItsOriginAndClock() {
        let recorder = RecordingHostEventRecorder()
        let telemetry = HostLoadTelemetry(recorder: recorder, origin: Self.origin, monotonicMilliseconds: { 77 })

        telemetry.record { origin, now in
            .runtimeBoot(
                outcome: .error,
                durationMilliseconds: 7,
                origin: origin,
                monotonicMilliseconds: now
            )
        }

        #expect(recorder.events.count == 1)
        #expect(recorder.events.first?.extras["event.monotonic_ms"] == .integer(77))
        #expect(recorder.events.first?.extras["host.app_build"] == .integer(456))
        #expect(recorder.events.first?.tags["outcome"] == "error")
    }

    /// A refresh scheduled from inside a page open must not take the page's session.
    @Test func aContextReplacesTheOneItRunsInsideAndReachesTasksItStarts() async {
        let telemetry = HostLoadTelemetry(recorder: RecordingHostEventRecorder())
        let session = UUID()

        let (open, refresh) = await telemetry.withContext(session: session, trigger: .open) {
            let open = await Task { telemetry.context }.value
            let refresh = await telemetry.withContext(session: nil, trigger: .refresh) { telemetry.context }
            return (open, refresh)
        }

        #expect(open == HostLoadContext(sessionID: session, trigger: .open))
        #expect(refresh == HostLoadContext(sessionID: nil, trigger: .refresh))
        #expect(telemetry.context == nil)
    }

    @Test func aCacheHitIsFirstOncePerSessionOrPerTriggerOutsideOne() async {
        let telemetry = HostLoadTelemetry(recorder: RecordingHostEventRecorder(), cacheHits: HostCacheHitLedger())
        let session = UUID()

        let inSession = await telemetry.withContext(session: session, trigger: .open) {
            [telemetry.isFirstCacheHit(of: "host.product.resolved", productID: "peopl.testnet"),
             telemetry.isFirstCacheHit(of: "host.product.resolved", productID: "peopl.testnet"),
             telemetry.isFirstCacheHit(of: "host.bulletin.fetch", productID: "peopl.testnet")]
        }
        let prewarm = await telemetry.withContext(session: nil, trigger: .prewarm) {
            [telemetry.isFirstCacheHit(of: "host.product.resolved", productID: "peopl.testnet"),
             telemetry.isFirstCacheHit(of: "host.product.resolved", productID: "peopl.testnet")]
        }

        #expect(inSession == [true, false, true])
        #expect(prewarm == [true, false])
        #expect(!telemetry.isFirstCacheHit(of: "host.product.resolved", productID: "other.testnet"))
    }

    @Test func cardTappedSaysWhetherTheTapOpenedOrReusedAPage() throws {
        let hostSessionID = try #require(UUID(uuidString: "6564F4CC-E69C-4052-BD4A-433F66790125"))

        let reused = HostEvent.pocketCardTapped(
            productID: "peopl.testnet",
            outcome: .reused,
            source: .pocketCard,
            durationMilliseconds: 180,
            hostSessionID: hostSessionID,
            origin: Self.origin,
            monotonicMilliseconds: 9
        )
        let opened = HostEvent.pocketCardTapped(
            productID: "peopl.testnet",
            outcome: .opened,
            source: .deeplink,
            durationMilliseconds: 35,
            hostSessionID: nil,
            origin: Self.origin,
            monotonicMilliseconds: 9
        )

        #expect(reused.name == "host.pocket.card_tapped")
        #expect(reused.tags["outcome"] == "reused")
        #expect(reused.tags["open.source"] == "pocket_card")
        #expect(reused.tags["product.id"] == "peopl.testnet")
        #expect(reused.tags["host.session_id"] == "6564f4cc-e69c-4052-bd4a-433f66790125")
        #expect(reused.durationMilliseconds == 180)
        #expect(reused.extras["reuse_ms"] == .integer(180))
        #expect(opened.tags["outcome"] == "opened")
        #expect(opened.tags["open.source"] == "deeplink")
        #expect(opened.durationMilliseconds == 35)
        #expect(opened.extras["reuse_ms"] == nil)
    }

    @Test func aTapReadsBothClocksAndMintsTheSessionItsPageUses() {
        let telemetry = HostLoadTelemetry(
            recorder: RecordingHostEventRecorder(),
            monotonicMilliseconds: { 42 },
            epochMilliseconds: { 1_793_900_000_000 }
        )

        let tap = telemetry.beginOpen(source: .pocketCard)

        #expect(tap?.monotonicMilliseconds == 42)
        #expect(tap?.openedAtEpochMilliseconds == 1_793_900_000_000)
        #expect(tap?.source == .pocketCard)
        #expect(HostLoadTelemetry.disabled.beginOpen(source: .pocketCard) == nil)
    }

    @Test func aNoopRecorderNeverBuildsTheEvent() {
        var built = false

        NoopHostEventRecorder().record({
            built = true
            return HostEvent(name: "host.test", tags: [:], extras: [:])
        }())

        #expect(!built)
    }

    private static let origin = HostEventOrigin(
        launchID: UUID(),
        osVersion: "26.5.1",
        appVersion: "1.2.3",
        appBuild: 456,
        appVariant: "nightly"
    )
}
