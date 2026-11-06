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

    @Test func webViewCreatedCarriesItsCorrelationIDAsATagAndAnExtra() {
        let event = HostEvent.webViewCreated(
            hostSessionID: HostEventFixtures.session,
            origin: HostEventFixtures.origin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 84
        )

        #expect(event == HostEvent(
            name: "host.webview.created",
            tags: [
                "host.app_variant": "nightly",
                "host.platform": "ios",
                "host.native_app_version": "1.2.3",
                "truapi.host_version": "0.9.0",
                "host.session_id": HostEventFixtures.sessionValue
            ],
            extras: [
                "host.app_build": .integer(456),
                "event.monotonic_ms": .integer(84),
                "host.session_id": .string(HostEventFixtures.sessionValue)
            ]
        ))
    }

    /// The whole event, so a stray or missing key breaks the cross-platform contract here first.
    @Test func productResolvedIsAStepUnderItsSession() {
        let event = HostEvent.productResolved(
            productID: "peopl.testnet",
            outcome: .ok,
            trigger: .open,
            durationMilliseconds: 555,
            hostSessionID: HostEventFixtures.session,
            origin: HostEventFixtures.origin,
            monotonicMilliseconds: 2_092
        )

        #expect(event == HostEvent(
            name: "host.product.resolved",
            tags: HostEventFixtures.stepTags(outcome: "ok", session: true).merging([
                "product.id": "peopl.testnet",
                "load.trigger": "open"
            ]) { _, new in new },
            extras: HostEventFixtures.stepExtras(duration: 555, monotonic: 2_092, session: true),
            durationMilliseconds: 555
        ))
    }

    @Test func bulletinFetchOutsideASessionKeepsItsNumbersTyped() {
        let event = HostEvent.bulletinFetch(
            domain: "app.peopl.testnet",
            outcome: .ok,
            trigger: .refresh,
            durationMilliseconds: 3_803,
            contentHashMilliseconds: 268,
            bytes: 9_497_026,
            hostSessionID: nil,
            origin: HostEventFixtures.origin,
            monotonicMilliseconds: 6_170
        )

        #expect(event == HostEvent(
            name: "host.bulletin.fetch",
            tags: HostEventFixtures.stepTags(outcome: "ok", session: false).merging([
                "load.trigger": "refresh"
            ]) { _, new in new },
            extras: HostEventFixtures.stepExtras(duration: 3_803, monotonic: 6_170, session: false).merging([
                "dotns.domain": .string("app.peopl.testnet"),
                "dotns.content_hash_ms": .integer(268),
                "bulletin.bytes": .integer(9_497_026)
            ]) { _, new in new },
            durationMilliseconds: 3_803
        ))
    }

    @Test func anAppliedArchiveHasNoContentHashReadOrBytes() {
        let event = HostEvent.bulletinFetch(
            domain: "app.peopl.testnet",
            outcome: .cacheHit,
            trigger: .prewarm,
            durationMilliseconds: 1,
            contentHashMilliseconds: nil,
            bytes: nil,
            hostSessionID: nil,
            origin: HostEventFixtures.origin,
            monotonicMilliseconds: 683
        )

        #expect(event.extras["dotns.content_hash_ms"] == nil)
        #expect(event.extras["bulletin.bytes"] == nil)
        #expect(event.tags["outcome"] == "cache_hit")
    }

    @Test func aDisabledLoadTelemetryNeverBuildsTheEvent() {
        var built = false

        HostLoadTelemetry.disabled.record { origin, now in
            built = true
            return .productResolved(
                productID: "peopl.testnet",
                outcome: .ok,
                trigger: .open,
                durationMilliseconds: 0,
                hostSessionID: nil,
                origin: origin,
                monotonicMilliseconds: now
            )
        }

        #expect(!built)
    }

    /// A Release build hands the telemetry a recorder that drops everything; it must time nothing for it.
    @Test func aDroppingRecorderDisablesTheTelemetry() async {
        let telemetry = HostLoadTelemetry(recorder: NoopHostEventRecorder())

        let context = await telemetry.withContext(session: UUID(), trigger: .open) { telemetry.context }

        #expect(!telemetry.isEnabled)
        #expect(telemetry.now() == nil)
        #expect(telemetry.beginOpen(source: .pocketCard) == nil)
        #expect(context == nil)
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
        let telemetry = HostLoadTelemetry(
            recorder: recorder,
            origin: HostEventFixtures.origin,
            monotonicMilliseconds: { 77 }
        )

        telemetry.record { origin, now in
            .productResolved(
                productID: "peopl.testnet",
                outcome: .error,
                trigger: .open,
                durationMilliseconds: 7,
                hostSessionID: nil,
                origin: origin,
                monotonicMilliseconds: now
            )
        }

        #expect(recorder.events.count == 1)
        #expect(recorder.events.first?.extras["event.monotonic_ms"] == .integer(77))
        #expect(recorder.events.first?.extras["app.launch_id"] == .string(HostEventFixtures.launchValue))
        #expect(recorder.events.first?.tags["host.os_version"] == "26.5.1")
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

    /// In a session each product reports one step whatever its outcome, except a failure, which never claims;
    /// outside one only cache hits are thinned, once per trigger.
    @Test func aStepReportsOncePerSessionAndACacheHitOncePerTriggerOutsideOne() async {
        let telemetry = HostLoadTelemetry(recorder: RecordingHostEventRecorder(), cacheHits: HostCacheHitLedger())
        let step = "host.product.resolved"

        let inSession = await telemetry.withContext(session: UUID(), trigger: .open) {
            [
                telemetry.shouldReport(step, productID: "peopl.testnet", outcome: .error),
                telemetry.shouldReport(step, productID: "peopl.testnet", outcome: .ok),
                telemetry.shouldReport(step, productID: "peopl.testnet", outcome: .cacheHit),
                telemetry.shouldReport(step, productID: "peopl.testnet", outcome: .error),
                telemetry.shouldReport("host.bulletin.fetch", productID: "peopl.testnet", outcome: .cacheHit)
            ]
        }
        let prewarm = await telemetry.withContext(session: nil, trigger: .prewarm) {
            [
                telemetry.shouldReport(step, productID: "peopl.testnet", outcome: .ok),
                telemetry.shouldReport(step, productID: "peopl.testnet", outcome: .ok),
                telemetry.shouldReport(step, productID: "peopl.testnet", outcome: .cacheHit),
                telemetry.shouldReport(step, productID: "peopl.testnet", outcome: .cacheHit)
            ]
        }

        #expect(inSession == [true, true, false, true, true])
        #expect(prewarm == [true, true, true, false])
        #expect(!telemetry.shouldReport(step, productID: "other.testnet", outcome: .ok))
    }

    @Test func aTapReadsBothClocks() {
        let telemetry = HostLoadTelemetry(
            recorder: RecordingHostEventRecorder(),
            monotonicMilliseconds: { 42 },
            epochMilliseconds: { 1_793_900_000_000 }
        )

        #expect(telemetry.beginOpen(source: .pocketCard) == HostOpenTap(
            source: .pocketCard,
            openedAtEpochMilliseconds: 1_793_900_000_000,
            monotonicMilliseconds: 42
        ))
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
}

/// A fixed origin, session and the shared step fields, so tests compare whole events against literals.
enum HostEventFixtures {
    static let launchValue = "18a5320e-3a4d-46df-ad7a-ebba4ef3ee3f"
    static let sessionValue = "6564f4cc-e69c-4052-bd4a-433f66790125"
    // swiftlint:disable:next force_unwrapping
    static let session = UUID(uuidString: sessionValue)!

    static let origin = HostEventOrigin(
        // swiftlint:disable:next force_unwrapping
        launchID: UUID(uuidString: launchValue)!,
        osVersion: "26.5.1",
        appVersion: "1.2.3",
        appBuild: 456,
        appVariant: "nightly"
    )

    static func stepTags(outcome: String, session: Bool) -> [String: String] {
        var tags = [
            "host.app_variant": "nightly",
            "host.platform": "ios",
            "host.native_app_version": "1.2.3",
            "host.os_version": "26.5.1",
            "outcome": outcome
        ]
        tags["host.session_id"] = session ? sessionValue : nil
        return tags
    }

    static func stepExtras(duration: Int, monotonic: Int, session: Bool) -> [String: HostEventValue] {
        var extras: [String: HostEventValue] = [
            "host.app_build": .integer(456),
            "event.monotonic_ms": .integer(monotonic),
            "duration_ms": .integer(duration),
            "app.launch_id": .string(launchValue)
        ]
        extras["host.session_id"] = session ? .string(sessionValue) : nil
        return extras
    }
}
