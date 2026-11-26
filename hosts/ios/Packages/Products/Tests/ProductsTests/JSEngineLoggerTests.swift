import Testing
@testable import Products

struct JSEngineLoggerTests {
    /// Console output keeps its own level and carries no `[truapi.*]` tag, so none of it reaches Sentry.
    @Test func consoleLinesKeepTheirOwnLevelAndCarryNoTag() {
        let logger = RecordingLogger()
        let jsLogger = JSEngineLogger(logger: logger)

        jsLogger.handle(body: ["level": "log", "message": "rendered"])
        jsLogger.handle(body: ["level": "warn", "message": "slow follow"])
        jsLogger.handle(body: ["level": "error", "message": "No answer within 45000 ms"])

        #expect(logger.lines == [
            .debug("JS: rendered"),
            .warning("JS: slow follow"),
            .error("JS: No answer within 45000 ms")
        ])
    }
}
