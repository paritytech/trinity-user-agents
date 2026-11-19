import Testing
@testable import Products

struct JSEngineLoggerTests {
    /// Tagged lines reach Sentry, so product console warnings and errors leave the device and the
    /// rest stays on it.
    @Test func onlyConsoleWarningsAndErrorsAreTaggedForSentry() {
        let logger = RecordingLogger()
        let jsLogger = JSEngineLogger(logger: logger)

        jsLogger.handle(body: ["level": "log", "message": "rendered"])
        jsLogger.handle(body: ["level": "warn", "message": "slow follow"])
        jsLogger.handle(body: ["level": "error", "message": "No answer within 45000 ms"])

        #expect(logger.lines == [
            .debug("JS: rendered"),
            .warning("[truapi.product] slow follow"),
            .error("[truapi.product] No answer within 45000 ms")
        ])
    }

    /// A console line can carry a whole payload, and only the first 512 characters leave the device.
    @Test func sentLinesAreCappedAt512Characters() {
        let logger = RecordingLogger()

        JSEngineLogger(logger: logger).handle(body: ["level": "error", "message": String(repeating: "x", count: 600)])

        #expect(logger.lines == [
            .error("[truapi.product] " + String(repeating: "x", count: 512))
        ])
    }

    /// An uncaught error line leaves the device, and the script or page URL of a browsed page can
    /// carry a token, so the line names the message only.
    @Test func uncaughtErrorsAreSentWithoutTheScriptURL() {
        let script = JSEngineLogger(logger: RecordingLogger()).getScript().content

        #expect(!script.contains("e.filename") && !script.contains("e.lineno"))
    }
}
