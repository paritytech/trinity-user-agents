import IssueMonitoring
import SDKLogger
import TrUAPIHost

/// The level each core marker is logged at, read once from the core's own
/// table. The three levels here are exactly the ones `HostLogSinkDestination`
/// forwards, so a marker with no level stays on the device: either the core
/// keeps it local because its detail describes what the person is doing, or
/// the table does not list it and nothing downstream knows what it carries.
private let exportedCoreMarkerLevels: [String: HostLogLevel] = coreMarkers()
    .compactMapValues { markerClass -> HostLogLevel? in
        switch markerClass {
        case .local:
            nil
        case .export(severity: .info):
            .info
        case .export(severity: .warn):
            .warning
        case .export(severity: .error):
            .error
        }
    }

extension LoggerProtocol {
    /// Logs one core line at the severity its marker carries, which is what
    /// decides whether the line leaves the device and how loud it is when it
    /// lands.
    func logCoreLine(marker: String, detail: String) {
        let line = "[truapi.core] \(marker): \(detail)"

        switch exportedCoreMarkerLevels[marker] {
        case .info:
            info(line)
        case .warning:
            warning(line)
        case .error:
            error(line)
        case nil:
            debug(line)
        }
    }
}
