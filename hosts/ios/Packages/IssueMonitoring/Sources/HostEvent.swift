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

    public static func productResolved(
        productID: String,
        outcome: HostEventOutcome,
        durationMilliseconds: Int,
        hostSessionID: UUID?,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        step(
            name: "host.product.resolved",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: hostSessionID,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: ["product.id": productID]
        )
    }

    public static func bulletinFetch(
        domain: String,
        outcome: HostEventOutcome,
        durationMilliseconds: Int,
        contentHashMilliseconds: Int,
        bytes: Int?,
        hostSessionID: UUID?,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        var extras = [
            "dotns.domain": HostEventValue.string(domain),
            "dotns.content_hash_ms": .integer(contentHashMilliseconds)
        ]
        extras["bulletin.bytes"] = bytes.map(HostEventValue.integer)

        return step(
            name: "host.bulletin.fetch",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: hostSessionID,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: [:],
            extras: extras
        )
    }

    public static func rpcConnected(
        chain: String,
        outcome: HostEventOutcome,
        durationMilliseconds: Int,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        step(
            name: "host.rpc.connected",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: nil,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: ["chain": chain]
        )
    }

    public static func pocketFaceLoaded(
        productID: String,
        outcome: HostEventOutcome,
        durationMilliseconds: Int,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        step(
            name: "host.pocket.face_loaded",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: nil,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: ["product.id": productID]
        )
    }

    public static func webViewPageLoaded(
        hostSessionID: UUID,
        productID: String,
        outcome: HostEventOutcome,
        durationMilliseconds: Int,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        step(
            name: "host.webview.page_loaded",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: hostSessionID,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: ["product.id": productID]
        )
    }
}

public enum HostEventOutcome: String, Sendable {
    case ok
    case error
    case cacheHit = "cache_hit"
}

private extension HostEvent {
    static func step(
        name: String,
        outcome: HostEventOutcome,
        durationMilliseconds: Int,
        hostSessionID: UUID?,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int,
        tags: [String: String],
        extras: [String: HostEventValue] = [:]
    ) -> HostEvent {
        var stepTags = tags
        stepTags["outcome"] = outcome.rawValue

        var stepExtras = extras
        stepExtras["duration_ms"] = .integer(durationMilliseconds)
        stepExtras["host.session_id"] = hostSessionID.map { HostEventValue.string($0.uuidString.lowercased()) }

        return make(
            name: name,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: stepTags,
            extras: stepExtras
        )
    }
}
