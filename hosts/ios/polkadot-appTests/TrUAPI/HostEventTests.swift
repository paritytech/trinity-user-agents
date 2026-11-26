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
            monotonicMilliseconds: 84,
            loadAttempt: 1
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
        let tap = HostOpenTap(source: .pocketCard, openedAt: Date(), openedAtMonotonicMilliseconds: 1_000)

        let event = try HostEvent.webViewCreated(
            hostSessionID: #require(UUID(uuidString: "d93fe846-8f3c-4cfa-a440-402b6f7b43d3")),
            origin: origin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 2_500,
            loadAttempt: 1,
            openTap: tap
        )

        #expect(event.tags["open.source"] == "pocket_card")
        #expect(event.extras["tap_ms"] == .integer(1500))
    }

    @Test func aTapMintsAFreshSessionIDEachTime() {
        let first = HostOpenTap(source: .pocketCard, openedAt: Date())
        let second = HostOpenTap(source: .pocketCard, openedAt: Date())

        #expect(first.sessionID != second.sessionID)
    }

    @Test func pocketOpenedCarriesTheIDMintedAtTheTap() throws {
        let sessionID = try #require(UUID(uuidString: "D93FE846-8F3C-4CFA-A440-402B6F7B43D3"))
        let tap = HostOpenTap(source: .deeplink, openedAt: Date(), sessionID: sessionID)

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
            tap: HostOpenTap(source: .pocketCard, openedAt: Date()),
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
        #expect(joined.openedAtMonotonicMilliseconds == tap.openedAtMonotonicMilliseconds)
        #expect(event.tags["host.session_id"] == "7ab1a66b-a1bb-41af-899f-d74d1d1f64c7")
        #expect(event.tags["open.reused"] == "true")
    }

    @Test func pageVisibleAfterATapMeasuresTheWebViewAndTheTap() throws {
        let tap = HostOpenTap(source: .pocketCard, openedAt: Date(), openedAtMonotonicMilliseconds: 100)

        let event = try HostEvent.pageVisible(
            hostSessionID: #require(UUID(uuidString: "d93fe846-8f3c-4cfa-a440-402b6f7b43d3")),
            origin: testOrigin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 1_600,
            webViewMilliseconds: 400,
            loadAttempt: 1,
            openTap: tap
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
            webViewMilliseconds: 400,
            loadAttempt: 1
        )

        #expect(event.tags["open.source"] == nil)
        #expect(event.extras["webview_ms"] == .integer(400))
        #expect(event.extras["tap_ms"] == nil)
    }

    /// The wall clock moved back between the tap and the commit, as a network time correction does. `tap_ms` is
    /// read on the monotonic clock, so the open still measures what it took rather than a clamped zero.
    @Test func aWallClockCorrectionDoesNotChangeTheTapMeasurement() {
        let tap = HostOpenTap(
            source: .pocketCard,
            openedAt: Date().addingTimeInterval(3_600),
            openedAtMonotonicMilliseconds: 100
        )

        let event = HostEvent.pageVisible(
            hostSessionID: UUID(),
            origin: testOrigin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 1_600,
            webViewMilliseconds: 400,
            loadAttempt: 1,
            openTap: tap
        )

        #expect(event.extras["tap_ms"] == .integer(1_500))
    }

    /// Both spans read the one monotonic clock, so neither is clamped: a negative span is sent as measured
    /// rather than as a zero that reads as the fastest possible load.
    @Test func pageVisibleSendsBothSpansAsMeasured() {
        let tap = HostOpenTap(source: .pocketCard, openedAt: Date(), openedAtMonotonicMilliseconds: 2_000)

        let event = HostEvent.pageVisible(
            hostSessionID: UUID(),
            origin: testOrigin,
            truapiHostVersion: "0.9.0",
            monotonicMilliseconds: 1_600,
            webViewMilliseconds: -50,
            loadAttempt: 1,
            openTap: tap
        )

        #expect(event.extras["webview_ms"] == .integer(-50))
        #expect(event.extras["tap_ms"] == .integer(-400))
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
