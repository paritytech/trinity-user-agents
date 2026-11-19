import IssueMonitoring
import os

private let attachedHostLogSink = OSAllocatedUnfairLock<HostLogSink?>(initialState: nil)

extension Logger {
    /// The sink `attach(sink:)` added, or nil when the build sends nothing off the device. Core events
    /// go to it directly, so their attributes arrive as fields rather than inside a line.
    var hostLogSink: HostLogSink? {
        attachedHostLogSink.withLock { $0 }
    }

    /// Sends `[truapi.*]` lines at info and above to `sink`, and keeps it for `hostLogSink`.
    func attach(sink: HostLogSink) {
        attachedHostLogSink.withLock { $0 = sink }
        log.addDestination(HostLogSinkDestination(sink: sink))
    }
}
