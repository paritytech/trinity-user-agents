import Foundation

/// An extra's value, kept typed so numbers reach Sentry as numbers.
public enum HostEventValue: Sendable, Equatable {
    case string(String)
    case integer(Int)
    case double(Double)
    case boolean(Bool)
}

/// A native host event: tags are indexed, extras are not, and `fingerprint` is
/// what the reporting backend groups on, so one name stays one group.
public struct HostEvent: Sendable, Equatable {
    public let name: String
    public let level: HostLogLevel
    public let fingerprint: [String]
    public let tags: [String: String]
    public let extras: [String: HostEventValue]
    public let counters: [String: Int]

    /// `fingerprint` is the key the backend groups on and defaults to the name
    /// alone. An event passes its own when its grouping is finer than the name,
    /// which a name derived from other fields cannot express. `counters` reaches
    /// the backend as a structured context rather than as extras.
    public init(
        name: String,
        level: HostLogLevel = .info,
        fingerprint: [String]? = nil,
        tags: [String: String],
        extras: [String: HostEventValue],
        counters: [String: Int] = [:]
    ) {
        let grouping = fingerprint ?? []
        self.name = name
        self.level = level
        self.fingerprint = grouping.isEmpty ? [name] : grouping
        self.tags = tags
        self.extras = extras
        self.counters = counters
    }

    /// An event carrying the fields every host event shares; `tags` and `extras` add its own.
    public static func make(
        name: String,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int,
        level: HostLogLevel = .info,
        fingerprint: [String]? = nil,
        tags: [String: String] = [:],
        extras: [String: HostEventValue] = [:],
        counters: [String: Int] = [:]
    ) -> HostEvent {
        var sharedTags = tags
        sharedTags["host.app_variant"] = origin.appVariant
        sharedTags["host.platform"] = "ios"
        sharedTags["host.native_app_version"] = origin.appVersion

        var sharedExtras = extras
        sharedExtras["host.app_build"] = origin.appBuild.map(HostEventValue.integer)
        sharedExtras["event.monotonic_ms"] = .integer(monotonicMilliseconds)

        return HostEvent(
            name: name,
            level: level,
            fingerprint: fingerprint,
            tags: sharedTags,
            extras: sharedExtras,
            counters: counters
        )
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
    public static func webViewCreated(
        hostSessionID: UUID,
        origin: HostEventOrigin,
        truapiHostVersion: String,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        make(
            name: "host.webview.created",
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: ["truapi.host_version": truapiHostVersion],
            extras: ["host.session_id": .string(hostSessionID.uuidString.lowercased())]
        )
    }
}
