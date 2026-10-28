import Foundation

public enum IssueMonitoringFactory {
    /// Returns the reporting service the current build can support. Callers stay
    /// unconditional: when no SDK is linked, or no DSN was injected, the result is a no-op.
    public static func createService(dsn: String) -> IssueMonitoringServiceProtocol {
        #if SENTRY_ENABLED
            guard !dsn.isEmpty else {
                return NoopIssueMonitoringService()
            }

            return SentryIssueMonitoringService(dsn: dsn)
        #else
            return NoopIssueMonitoringService()
        #endif
    }

    /// Whether this build links a reporting SDK; feature diagnostics are no-ops otherwise.
    public static var isReportingSupported: Bool {
        #if SENTRY_ENABLED
            true
        #else
            false
        #endif
    }

    /// Builds the immutable product bootstrap only when this build links reporting support.
    public static func createNativeHostTelemetry(
        hostSessionID: UUID,
        truapiHostVersion: @autoclosure () -> String
    ) -> NativeHostTelemetry? {
        guard isReportingSupported else { return nil }

        return NativeHostTelemetry(
            hostSessionID: hostSessionID,
            origin: .current,
            truapiHostVersion: truapiHostVersion()
        )
    }

    /// Records typed native host lifecycle events; builds without reporting support receive a no-op recorder.
    public static func createHostEventRecorder() -> HostEventRecording {
        #if SENTRY_ENABLED
            SentryHostEventRecorder()
        #else
            NoopHostEventRecorder()
        #endif
    }

    /// Writes a line of device readings to the given log once a minute in the foreground, and sends them
    /// as gauges when this build links reporting support.
    public static func createDeviceReadingsMonitor(log: @escaping (String) -> Void) -> DeviceReadingsMonitoring {
        DeviceReadingsMonitor { readings in
            log("Device readings: \(readings.logLine)")
            #if SENTRY_ENABLED
                readings.sendSentryGauges()
            #endif
        }
    }

    /// Reports through the SDK `IssueMonitoringServiceProtocol.setup()` started; a no-op until then and in Release.
    public static func createReporter() -> IssueReporting {
        #if SENTRY_ENABLED
            SentryIssueReporter()
        #else
            NoopIssueReporter()
        #endif
    }
}
