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

    @Test func webViewCreatedCarriesItsCorrelationIDAsAnExtra() throws {
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
        #expect(event.extras["host.session_id"] == .string("d93fe846-8f3c-4cfa-a440-402b6f7b43d3"))
        #expect(event.extras["event.monotonic_ms"] == .integer(84))
        #expect(event.extras["app.launch_id"] == nil)
        #expect(event.tags["host.os_version"] == nil)
    }

    @Test func productResolvedCarriesItsOutcomeDurationAndSession() throws {
        let hostSessionID = try #require(UUID(uuidString: "d93fe846-8f3c-4cfa-a440-402b6f7b43d3"))

        let event = HostEvent.productResolved(
            productID: "peopl.testnet",
            outcome: .ok,
            durationMilliseconds: 555,
            hostSessionID: hostSessionID,
            origin: Self.origin,
            monotonicMilliseconds: 2_092
        )

        #expect(event.name == "host.product.resolved")
        #expect(event.tags["product.id"] == "peopl.testnet")
        #expect(event.tags["outcome"] == "ok")
        #expect(event.tags["host.platform"] == "ios")
        #expect(event.tags["host.app_variant"] == "nightly")
        #expect(event.extras["duration_ms"] == .integer(555))
        #expect(event.extras["event.monotonic_ms"] == .integer(2_092))
        #expect(event.extras["host.app_build"] == .integer(456))
        #expect(event.extras["host.session_id"] == .string("d93fe846-8f3c-4cfa-a440-402b6f7b43d3"))
    }

    @Test func aStepOutsideAnySessionCarriesNoSessionID() {
        let event = HostEvent.productResolved(
            productID: "peopl.testnet",
            outcome: .error,
            durationMilliseconds: 12,
            hostSessionID: nil,
            origin: Self.origin,
            monotonicMilliseconds: 40
        )

        #expect(event.tags["outcome"] == "error")
        #expect(event.extras["host.session_id"] == nil)
    }

    @Test func bulletinFetchKeepsItsByteCountAndContentHashReadNumeric() {
        let downloaded = HostEvent.bulletinFetch(
            domain: "app.peopl.testnet",
            outcome: .ok,
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
        #expect(downloaded.extras["duration_ms"] == .integer(3_803))
        #expect(downloaded.extras["dotns.content_hash_ms"] == .integer(268))
        #expect(downloaded.extras["bulletin.bytes"] == .integer(9_497_026))

        let onDisk = HostEvent.bulletinFetch(
            domain: "app.peopl.testnet",
            outcome: .cacheHit,
            durationMilliseconds: 1,
            contentHashMilliseconds: 341,
            bytes: nil,
            hostSessionID: nil,
            origin: Self.origin,
            monotonicMilliseconds: 683
        )

        #expect(onDisk.tags["outcome"] == "cache_hit")
        #expect(onDisk.extras["bulletin.bytes"] == nil)
    }

    @Test func sharedStepsCarryNoSession() {
        let rpc = HostEvent.rpcConnected(
            chain: "people",
            outcome: .ok,
            durationMilliseconds: 436,
            origin: Self.origin,
            monotonicMilliseconds: 6_916
        )
        let face = HostEvent.pocketFaceLoaded(
            productID: "peopl.testnet",
            outcome: .cacheHit,
            durationMilliseconds: 24,
            origin: Self.origin,
            monotonicMilliseconds: 24
        )

        #expect(rpc.name == "host.rpc.connected")
        #expect(rpc.tags["chain"] == "people")
        #expect(rpc.extras["duration_ms"] == .integer(436))
        #expect(rpc.extras["host.session_id"] == nil)
        #expect(face.name == "host.pocket.face_loaded")
        #expect(face.tags["product.id"] == "peopl.testnet")
        #expect(face.tags["outcome"] == "cache_hit")
        #expect(face.extras["host.session_id"] == nil)
    }

    @Test func pageLoadedCarriesTheSessionItsPageReads() throws {
        let hostSessionID = try #require(UUID(uuidString: "6564F4CC-E69C-4052-BD4A-433F66790125"))

        let event = HostEvent.webViewPageLoaded(
            hostSessionID: hostSessionID,
            productID: "peopl.testnet",
            outcome: .ok,
            durationMilliseconds: 6_349,
            origin: Self.origin,
            monotonicMilliseconds: 1_830_576
        )

        #expect(event.name == "host.webview.page_loaded")
        #expect(event.tags["product.id"] == "peopl.testnet")
        #expect(event.tags["outcome"] == "ok")
        #expect(event.extras["duration_ms"] == .integer(6_349))
        #expect(event.extras["event.monotonic_ms"] == .integer(1_830_576))
        #expect(event.extras["host.session_id"] == .string("6564f4cc-e69c-4052-bd4a-433f66790125"))
    }

    @Test func aDisabledLoadTelemetryNeverBuildsTheEvent() {
        var built = false

        HostLoadTelemetry.disabled.record { origin, now in
            built = true
            return .rpcConnected(
                chain: "people",
                outcome: .ok,
                durationMilliseconds: 0,
                origin: origin,
                monotonicMilliseconds: now
            )
        }

        #expect(!built)
    }

    @Test func aDisabledLoadTelemetryReadsNoClockAndKeepsNoSession() {
        let telemetry = HostLoadTelemetry.disabled

        telemetry.openSession(productID: "peopl.testnet", sessionID: UUID())

        #expect(!telemetry.isEnabled)
        #expect(telemetry.now() == nil)
        #expect(telemetry.sessionID(forProductOrSubname: "peopl.testnet") == nil)
    }

    @Test func loadTelemetryStampsTheEventWithItsOriginAndClock() {
        let recorder = RecordingHostEventRecorder()
        let telemetry = HostLoadTelemetry(recorder: recorder, origin: Self.origin, monotonicMilliseconds: { 77 })

        telemetry.record { origin, now in
            .rpcConnected(
                chain: "bulletin",
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

    @Test func aSessionCoversItsProductAndSubnamesUntilClosed() {
        let sessions = HostSessionRegistry()
        let first = UUID()
        let second = UUID()

        sessions.open(productID: "peopl.testnet", sessionID: first)
        #expect(sessions.sessionID(forProductOrSubname: "peopl.testnet") == first)
        #expect(sessions.sessionID(forProductOrSubname: "app.peopl.testnet") == first)
        #expect(sessions.sessionID(forProductOrSubname: "otherpeopl.testnet") == nil)

        // A newer session for the same product outlives the older one closing.
        sessions.open(productID: "peopl.testnet", sessionID: second)
        sessions.close(productID: "peopl.testnet", sessionID: first)
        #expect(sessions.sessionID(forProductOrSubname: "peopl.testnet") == second)

        sessions.close(productID: "peopl.testnet", sessionID: second)
        #expect(sessions.sessionID(forProductOrSubname: "app.peopl.testnet") == nil)
    }

    @Test func aSubnameBelongsToTheLongestProductItEndsWith() {
        let sessions = HostSessionRegistry()
        let outer = UUID()
        let inner = UUID()

        sessions.open(productID: "testnet", sessionID: outer)
        sessions.open(productID: "peopl.testnet", sessionID: inner)

        #expect(sessions.sessionID(forProductOrSubname: "app.peopl.testnet") == inner)
        #expect(sessions.sessionID(forProductOrSubname: "app.other.testnet") == outer)
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
