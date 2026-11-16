import Foundation

/// Where a host event lands: an info event is a log line and anything above it
/// is a captured event, so the level a mapping picks is the product its signal
/// is queried in. See `docs/design/host-telemetry.md`.
enum HostEventRoute: Equatable {
    /// A log line whose body is the event name. A log has no grouping, so the
    /// fingerprint is dropped, and it has no contexts, so counters are too.
    case log(body: String, attributes: [String: HostEventValue])
    case capture
}

extension HostEvent {
    /// The value of the `tag` attribute on every log line a host event becomes,
    /// which keeps them apart from the `truapi.` lines on that attribute.
    static let logTag = "host.telemetry"

    var route: HostEventRoute {
        switch level {
        case .info:
            .log(body: name, attributes: logAttributes)
        case .warning,
             .error:
            .capture
        }
    }

    /// Tags and extras flattened into one namespace. A key spelled as both
    /// resolves to the extra, as Sentry resolves it, and `tag` and `event.name`
    /// are set last so neither can be overwritten.
    private var logAttributes: [String: HostEventValue] {
        var attributes = tags.mapValues(HostEventValue.string)
        attributes.merge(extras) { _, extra in extra }
        attributes["tag"] = .string(Self.logTag)
        attributes["event.name"] = .string(name)
        return attributes
    }
}
