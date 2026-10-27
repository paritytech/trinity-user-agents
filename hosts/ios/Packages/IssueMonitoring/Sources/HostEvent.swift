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

    /// The event for the app process finishing launch.
    public static func processStarted(
        origin: HostEventOrigin,
        truapiHostVersion: String,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        HostEvent(
            name: "host.process.started",
            tags: tags(origin: origin, truapiHostVersion: truapiHostVersion).merging([
                "host.os_version": origin.osVersion
            ]) { _, value in value },
            extras: appMetadata(origin: origin).merging([
                "app.launch_id": .string(origin.launchID.uuidString.lowercased()),
                "event.monotonic_ms": .integer(monotonicMilliseconds)
            ]) { _, value in value }
        )
    }

    /// The event for a product WebView; `hostSessionID` is the id its page reads from `__trinityHostTelemetry`.
    public static func webViewCreated(
        hostSessionID: UUID,
        origin: HostEventOrigin,
        truapiHostVersion: String,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        HostEvent(
            name: "host.webview.created",
            tags: tags(origin: origin, truapiHostVersion: truapiHostVersion),
            extras: appMetadata(origin: origin).merging([
                "host.session_id": .string(hostSessionID.uuidString.lowercased()),
                "event.monotonic_ms": .integer(monotonicMilliseconds)
            ]) { _, value in value }
        )
    }
}

private extension HostEvent {
    static func tags(origin: HostEventOrigin, truapiHostVersion: String) -> [String: String] {
        var tags = [
            "host.app_variant": origin.appVariant,
            "host.platform": "ios",
            "truapi.host_version": truapiHostVersion
        ]
        tags["host.native_app_version"] = origin.appVersion
        return tags
    }

    static func appMetadata(origin: HostEventOrigin) -> [String: HostEventValue] {
        var extras: [String: HostEventValue] = [:]
        extras["host.app_build"] = origin.appBuild.map(HostEventValue.integer)
        return extras
    }
}
