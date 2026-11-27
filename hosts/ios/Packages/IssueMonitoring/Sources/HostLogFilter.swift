import Foundation

/// What the gate decided about one log.
enum HostLogVerdict: Equatable {
    /// Send it. `dropped` counts the lines the window that just closed turned
    /// away under the same key, and is zero when it turned none away.
    case send(dropped: Int)
    /// Do not send it.
    case drop
}

/// Decides which logs leave the device, at the last gate before the SDK sends
/// one.
///
/// A log goes out only when it carries a `tag` this host writes, so a line that
/// reaches the logger by a route nobody planned is turned away here rather than
/// sent. What is left sends at most `linesPerWindow` lines per `window` under
/// one key. A `truapi.core` line counts against its marker and a host event
/// against its name, so one noisy marker cannot hide the others. The first line
/// after a window that turned any away carries `droppedAttribute`, which counts
/// them, so a key stuck in a loop costs that many lines a minute and says how
/// many it stood in for.
final class HostLogFilter: @unchecked Sendable {
    static let linesPerWindow = 60
    static let window: TimeInterval = 60
    static let tagPrefix = "truapi."
    static let coreTag = "truapi.core"
    static let markerSeparator = ": "
    static let markerLimit = 96
    static let tagAttribute = "tag"
    static let eventNameAttribute = "event.name"
    static let droppedAttribute = "dropped"

    private struct Window {
        var start: TimeInterval
        var sent = 0
        var dropped = 0
    }

    private let now: () -> TimeInterval
    private let lock = NSLock()
    private var windows: [String: Window] = [:]

    init(now: @escaping () -> TimeInterval = { ProcessInfo.processInfo.systemUptime }) {
        self.now = now
    }

    /// Whether a log carrying `tag`, `body` and `eventName` may be sent.
    func verdict(tag: String?, body: String, eventName: String?) -> HostLogVerdict {
        guard let tag, Self.isOurs(tag) else { return .drop }
        return admit(key: Self.key(tag: tag, body: body, eventName: eventName))
    }

    /// Whether this host writes `tag`. A log carrying any other tag, or none,
    /// reached the logger by a route this host does not own.
    private static func isOurs(_ tag: String) -> Bool {
        tag == HostEvent.logTag || tag.hasPrefix(tagPrefix)
    }

    /// What a log counts against: its marker where it has one, else its tag.
    private static func key(tag: String, body: String, eventName: String?) -> String {
        if tag == coreTag, let marker = coreMarker(of: body) {
            return "\(tag) \(marker)"
        }
        if tag == HostEvent.logTag, let eventName, !eventName.isEmpty {
            return "\(tag) \(eventName)"
        }
        return tag
    }

    /// The marker a `truapi.core` body names. A core event's body is the marker
    /// on its own, and a core log line writes it before its fields, so a body
    /// naming none inside `markerLimit` names none at all and counts against
    /// its tag, which is what keeps one long line from growing the table.
    private static func coreMarker(of body: String) -> String? {
        let head = body.prefix(markerLimit)
        guard let separator = head.range(of: markerSeparator) else {
            return head.isEmpty || head.count == markerLimit ? nil : String(head)
        }
        guard separator.lowerBound > head.startIndex else { return nil }
        return String(head[..<separator.lowerBound])
    }

    /// Counts a line against its window.
    private func admit(key: String) -> HostLogVerdict {
        lock.withLock {
            let time = now()
            var window = windows[key] ?? Window(start: time)
            var closedWithDrops = 0
            if time - window.start >= Self.window {
                closedWithDrops = window.dropped
                window = Window(start: time)
            }
            defer { windows[key] = window }
            guard window.sent < Self.linesPerWindow else {
                window.dropped += 1
                return .drop
            }
            window.sent += 1
            return .send(dropped: closedWithDrops)
        }
    }
}
