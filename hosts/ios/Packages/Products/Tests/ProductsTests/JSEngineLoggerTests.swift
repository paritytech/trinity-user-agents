import Testing
@testable import Products

struct JSEngineLoggerTests {
    /// Console output stays on the device, so every level keeps its own and no line carries a
    /// `[truapi.*]` tag.
    @Test func consoleLinesStayOnTheDeviceAtTheirOwnLevel() {
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
