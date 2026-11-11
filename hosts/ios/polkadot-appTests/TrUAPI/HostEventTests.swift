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

    @Test func webViewCreatedCarriesItsCorrelationIDAsASearchableTag() throws {
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
        #expect(event.extras["event.monotonic_ms"] == .integer(84))
        #expect(event.extras["app.launch_id"] == nil)
        #expect(event.extras["tap_ms"] == nil)
        #expect(event.tags["open.source"] == nil)
        #expect(event.tags["host.os_version"] == nil)
    }

    @Test func webViewCreatedAfterATapSaysWhatOpenedItAndHowLongTheHostTook() throws {
        let origin = HostEventOrigin(
            launchID: UUID(),
            osVersion: "26.5",
            appVersion: nil,
            appBuild: nil,
            appVariant: "debug"
        )
        let tap = HostOpenTap(source: .pocketCard, openedAt: Date(timeIntervalSince1970: 1_793_900_000.123))

        let event = try HostEvent.webViewCreated(
            hostSessionID: #require(UUID(uuidString: "d93fe846-8f3c-4cfa-a440-402b6f7b43d3")),
            origin: origin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 84,
            openTap: tap,
            now: Date(timeIntervalSince1970: 1_793_900_001.623)
        )

        #expect(event.tags["open.source"] == "pocket_card")
        #expect(event.extras["tap_ms"] == .integer(1500))
    }

    @Test func aTapMintsAFreshSessionIDEachTime() {
        let first = HostOpenTap(source: .pocketCard)
        let second = HostOpenTap(source: .pocketCard)

        #expect(first.sessionID != second.sessionID)
    }

    @Test func pocketOpenedCarriesTheIDMintedAtTheTap() throws {
        let sessionID = try #require(UUID(uuidString: "D93FE846-8F3C-4CFA-A440-402B6F7B43D3"))
        let tap = HostOpenTap(source: .deeplink, sessionID: sessionID)

        let event = HostEvent.pocketOpened(
            tap: tap,
            reused: false,
            origin: testOrigin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 12
        )

        #expect(event.name == "host.pocket.opened")
        #expect(event.tags["host.session_id"] == "d93fe846-8f3c-4cfa-a440-402b6f7b43d3")
        #expect(event.tags["open.source"] == "deeplink")
        #expect(event.tags["open.reused"] == "false")
        #expect(event.extras["event.monotonic_ms"] == .integer(12))
        #expect(event.extras["tap_ms"] == nil)
    }

    @Test func pocketOpenedOnAReusedPageSaysSo() {
        let event = HostEvent.pocketOpened(
            tap: HostOpenTap(source: .pocketCard),
            reused: true,
            origin: testOrigin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 12
        )

        #expect(event.tags["open.source"] == "pocket_card")
        #expect(event.tags["open.reused"] == "true")
    }

    @Test func aReusedOpenCarriesTheSessionIDOfThePageItShows() throws {
        let pageSessionID = try #require(UUID(uuidString: "7AB1A66B-A1BB-41AF-899F-D74D1D1F64C7"))
        let tap = HostOpenTap(source: .pocketCard, openedAt: Date(timeIntervalSince1970: 1_793_900_000.123))

        let joined = tap.showing(sessionID: pageSessionID)
        let event = HostEvent.pocketOpened(
            tap: joined,
            reused: true,
            origin: testOrigin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 12
        )

        #expect(joined.source == tap.source)
        #expect(joined.openedAtEpochMilliseconds == tap.openedAtEpochMilliseconds)
        #expect(event.tags["host.session_id"] == "7ab1a66b-a1bb-41af-899f-d74d1d1f64c7")
        #expect(event.tags["open.reused"] == "true")
    }

    @Test func pageVisibleAfterATapMeasuresTheWebViewAndTheTap() throws {
        let tap = HostOpenTap(source: .pocketCard, openedAt: Date(timeIntervalSince1970: 1_793_900_000.123))

        let event = try HostEvent.pageVisible(
            hostSessionID: #require(UUID(uuidString: "d93fe846-8f3c-4cfa-a440-402b6f7b43d3")),
            origin: testOrigin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 900,
            webViewMilliseconds: 400,
            openTap: tap,
            now: Date(timeIntervalSince1970: 1_793_900_001.623)
        )

        #expect(event.name == "host.page.visible")
        #expect(event.tags["host.session_id"] == "d93fe846-8f3c-4cfa-a440-402b6f7b43d3")
        #expect(event.tags["open.source"] == "pocket_card")
        #expect(event.extras["webview_ms"] == .integer(400))
        #expect(event.extras["tap_ms"] == .integer(1500))
    }

    @Test func pageVisibleWithoutATapHasNoTapMeasurement() throws {
        let event = try HostEvent.pageVisible(
            hostSessionID: #require(UUID(uuidString: "d93fe846-8f3c-4cfa-a440-402b6f7b43d3")),
            origin: testOrigin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 900,
            webViewMilliseconds: 400
        )

        #expect(event.tags["open.source"] == nil)
        #expect(event.extras["webview_ms"] == .integer(400))
        #expect(event.extras["tap_ms"] == nil)
    }

    @Test func pageVisibleClampsATapClockedAfterItsCommit() {
        let tap = HostOpenTap(source: .pocketCard, openedAt: Date(timeIntervalSince1970: 1_793_900_002))

        let event = HostEvent.pageVisible(
            hostSessionID: UUID(),
            origin: testOrigin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 900,
            webViewMilliseconds: 400,
            openTap: tap,
            now: Date(timeIntervalSince1970: 1_793_900_001)
        )

        #expect(event.extras["tap_ms"] == .integer(0))
    }

    @Test func aNoopRecorderNeverBuildsTheEvent() {
        var built = false

        NoopHostEventRecorder().record({
            built = true
            return HostEvent(name: "host.test", tags: [:], extras: [:])
        }())

        #expect(!built)
    }

    private var testOrigin: HostEventOrigin {
        HostEventOrigin(launchID: UUID(), osVersion: "26.5", appVersion: nil, appBuild: nil, appVariant: "debug")
    }
}
