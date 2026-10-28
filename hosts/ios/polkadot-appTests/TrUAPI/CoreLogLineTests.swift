import Testing
@testable import polkadot_app

struct CoreLogLineTests {
    /// Info lines go to Sentry, and a push notification body must never leave the device.
    @Test func onlyAllowedCoreMarkersAreLoggedAtInfo() {
        let logger = RecordingLogger()

        logger.logCoreLine(marker: "truapi.native.callback.push_notification", detail: "hello")
        logger.logCoreLine(marker: "truapi.tracing.warn", detail: "truapi::runtime: chain call failed")

        #expect(logger.lines == [
            .debug("[truapi.core] truapi.native.callback.push_notification: hello"),
            .info("[truapi.core] truapi.tracing.warn: truapi::runtime: chain call failed")
        ])
    }

    /// The markers the core addresses to a host are the reason it raises them.
    /// Reaching only the device log would make marking an event for the host
    /// cost it the reach an ordinary warning already has.
    @Test func coreMarkersAddressedToTheHostAreLoggedAtInfo() {
        let logger = RecordingLogger()

        logger.logCoreLine(marker: "truapi.chain.follow_budget_reached", detail: "four of four")
        logger.logCoreLine(marker: "truapi.core.started", detail: "truapi 0.0.0")
        logger.logCoreLine(marker: "truapi.ws_bridge.connection_open", detail: "one of eight")

        #expect(logger.lines == [
            .info("[truapi.core] truapi.chain.follow_budget_reached: four of four"),
            .info("[truapi.core] truapi.core.started: truapi 0.0.0"),
            .info("[truapi.core] truapi.ws_bridge.connection_open: one of eight")
        ])
    }
}
