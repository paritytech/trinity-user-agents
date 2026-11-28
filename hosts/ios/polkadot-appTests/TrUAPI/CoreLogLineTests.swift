import Testing
@testable import polkadot_app

struct CoreLogLineTests {
    /// A local marker is logged below the level the host log sink forwards,
    /// whether its detail describes what the person is doing or it only
    /// records that something went normally.
    @Test func aLocalMarkerStaysOnTheDevice() {
        let logger = RecordingLogger()

        logger.logCoreLine(marker: "truapi.native.callback.push_notification", detail: "hello")
        logger.logCoreLine(marker: "truapi.core.started", detail: "truapi 0.0.0")
        logger.logCoreLine(marker: "truapi.ws_bridge.connection_open", detail: "one of eight")

        #expect(logger.lines == [
            .debug("[truapi.core] truapi.native.callback.push_notification: hello"),
            .debug("[truapi.core] truapi.core.started: truapi 0.0.0"),
            .debug("[truapi.core] truapi.ws_bridge.connection_open: one of eight")
        ])
    }

    /// The severity comes from the core's table, so an error is alerted on
    /// separately rather than arriving as one more info line.
    @Test func anExportedMarkerIsLoggedAtItsOwnSeverity() {
        let logger = RecordingLogger()

        logger.logCoreLine(marker: "truapi.chain.follow_budget_reached", detail: "four of four")
        logger.logCoreLine(marker: "truapi.tracing.warn", detail: "truapi::runtime: retrying")
        logger.logCoreLine(marker: "truapi.tracing.error", detail: "truapi::runtime: chain call failed")

        #expect(logger.lines == [
            .warning("[truapi.core] truapi.chain.follow_budget_reached: four of four"),
            .warning("[truapi.core] truapi.tracing.warn: truapi::runtime: retrying"),
            .error("[truapi.core] truapi.tracing.error: truapi::runtime: chain call failed")
        ])
    }

    /// Nothing downstream knows what an unlisted marker carries, so it is
    /// treated as local rather than exported on the strength of its prefix.
    @Test func aMarkerTheTableDoesNotListStaysOnTheDevice() {
        let logger = RecordingLogger()

        logger.logCoreLine(marker: "truapi.chain.not_registered", detail: "whatever this is")

        #expect(logger.lines == [
            .debug("[truapi.core] truapi.chain.not_registered: whatever this is")
        ])
    }
}
