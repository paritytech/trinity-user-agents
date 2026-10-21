import Testing
@testable import polkadot_app

struct CoreLogLineTests {
    /// Info lines go to Sentry, and a push notification body must never leave the device.
    @Test func onlyCoreTracingWarningsAndErrorsAreLoggedAtInfo() {
        let logger = RecordingLogger()

        logger.logCoreLine(marker: "truapi.native.callback.push_notification", detail: "hello")
        logger.logCoreLine(marker: "truapi.tracing.warn", detail: "truapi::runtime: chain call failed")

        #expect(logger.lines == [
            .debug("[truapi.core] truapi.native.callback.push_notification: hello"),
            .info("[truapi.core] truapi.tracing.warn: truapi::runtime: chain call failed")
        ])
    }
}
