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

    @Test func aNoopRecorderNeverBuildsTheEvent() {
        var built = false

        NoopHostEventRecorder().record({
            built = true
            return HostEvent(name: "host.test", tags: [:], extras: [:])
        }())

        #expect(!built)
    }
}
