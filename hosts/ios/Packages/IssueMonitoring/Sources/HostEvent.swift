import Foundation

/// An extra's value, kept typed so numbers reach Sentry as numbers.
public enum HostEventValue: Sendable, Equatable {
    case string(String)
    case integer(Int)
    case double(Double)
    case boolean(Bool)
}

/// A native host lifecycle event: tags are indexed, extras are not.
public struct HostEvent: Sendable, Equatable {
    public let name: String
    public let tags: [String: String]
    public let extras: [String: HostEventValue]

    public init(name: String, tags: [String: String], extras: [String: HostEventValue]) {
        self.name = name
        self.tags = tags
        self.extras = extras
    }

    /// An event carrying the fields every host event shares; `tags` and `extras` add its own.
    public static func make(
        name: String,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int,
        tags: [String: String] = [:],
        extras: [String: HostEventValue] = [:]
    ) -> HostEvent {
        var sharedTags = tags
        sharedTags["host.app_variant"] = origin.appVariant
        sharedTags["host.platform"] = "ios"
        sharedTags["host.native_app_version"] = origin.appVersion

        var sharedExtras = extras
        sharedExtras["host.app_build"] = origin.appBuild.map(HostEventValue.integer)
        sharedExtras["event.monotonic_ms"] = .integer(monotonicMilliseconds)

        return HostEvent(name: name, tags: sharedTags, extras: sharedExtras)
    }

    /// The event for the app process finishing launch.
    public static func processStarted(
        origin: HostEventOrigin,
        truapiHostVersion: String,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        make(
            name: "host.process.started",
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: [
                "truapi.host_version": truapiHostVersion,
                "host.os_version": origin.osVersion
            ],
            extras: ["app.launch_id": .string(origin.launchID.uuidString.lowercased())]
        )
    }

    /// The event for a product WebView; `hostSessionID` is the id its page reads from `__trinityHostTelemetry`.
    /// When a tap opened the page, `tap_ms` is the host's share of the open: the tap to this WebView.
    public static func webViewCreated(
        hostSessionID: UUID,
        origin: HostEventOrigin,
        truapiHostVersion: String,
        monotonicMilliseconds: Int,
        openTap: HostOpenTap? = nil,
        now: Date = Date()
    ) -> HostEvent {
        var tags = [
            "truapi.host_version": truapiHostVersion,
            "host.session_id": hostSessionID.uuidString.lowercased()
        ]
        var extras: [String: HostEventValue] = [:]

        if let openTap {
            tags["open.source"] = openTap.source.rawValue
            extras["tap_ms"] = .integer(max(0, now.epochMilliseconds - openTap.openedAtEpochMilliseconds))
        }

        return make(
            name: "host.webview.created",
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: tags,
            extras: extras
        )
    }

    /// The event for a tap that opens a Pocket card, sent at the tap. `reused` is whether the tap took a page that
    /// was already open, in which case no later event carries `tap.sessionID`.
    public static func pocketOpened(
        tap: HostOpenTap,
        reused: Bool,
        origin: HostEventOrigin,
        truapiHostVersion: String,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        make(
            name: "host.pocket.opened",
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: [
                "truapi.host_version": truapiHostVersion,
                "host.session_id": tap.sessionID.uuidString.lowercased(),
                "open.source": tap.source.rawValue,
                "open.reused": reused ? "true" : "false"
            ]
        )
    }

    /// The event for the first visible commit of a session's main page. `webview_ms` is the WebView's creation to
    /// that commit; `tap_ms` is the tap to it, and only a session a tap opened has one.
    public static func pageVisible(
        hostSessionID: UUID,
        origin: HostEventOrigin,
        truapiHostVersion: String,
        monotonicMilliseconds: Int,
        webViewMilliseconds: Int,
        openTap: HostOpenTap? = nil,
        now: Date = Date()
    ) -> HostEvent {
        var tags = [
            "truapi.host_version": truapiHostVersion,
            "host.session_id": hostSessionID.uuidString.lowercased()
        ]
        var extras: [String: HostEventValue] = ["webview_ms": .integer(max(0, webViewMilliseconds))]

        if let openTap {
            tags["open.source"] = openTap.source.rawValue
            extras["tap_ms"] = .integer(max(0, now.epochMilliseconds - openTap.openedAtEpochMilliseconds))
        }

        return make(
            name: "host.page.visible",
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: tags,
            extras: extras
        )
    }
}
