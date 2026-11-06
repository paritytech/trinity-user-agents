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
    /// How long a step took; nil for a point event. Charted as a distribution named after the event.
    public let durationMilliseconds: Int?

    public init(
        name: String,
        tags: [String: String],
        extras: [String: HostEventValue],
        durationMilliseconds: Int? = nil
    ) {
        self.name = name
        self.tags = tags
        self.extras = extras
        self.durationMilliseconds = durationMilliseconds
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
        let session = hostSessionID.telemetryValue

        return make(
            name: "host.webview.created",
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: ["truapi.host_version": truapiHostVersion, "host.session_id": session],
            extras: ["host.session_id": .string(session)]
        )
    }

    public static func productResolved(
        productID: String,
        outcome: HostEventOutcome,
        trigger: HostLoadTrigger,
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
            tags: ["product.id": productID, "load.trigger": trigger.rawValue]
        )
    }

    public static func bulletinFetch(
        domain: String,
        outcome: HostEventOutcome,
        trigger: HostLoadTrigger,
        durationMilliseconds: Int,
        contentHashMilliseconds: Int?,
        bytes: Int?,
        hostSessionID: UUID?,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        var extras = ["dotns.domain": HostEventValue.string(domain)]
        extras["dotns.content_hash_ms"] = contentHashMilliseconds.map(HostEventValue.integer)
        extras["bulletin.bytes"] = bytes.map(HostEventValue.integer)

        return step(
            name: "host.bulletin.fetch",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: hostSessionID,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: ["load.trigger": trigger.rawValue],
            extras: extras
        )
    }
}

public enum HostEventOutcome: String, Sendable {
    case ok
    case error
    case cacheHit = "cache_hit"
    case timeout
    case noWorker = "no_worker"
    case abandoned
    case opened
    case reused
}

public enum HostOpenSource: String, Sendable {
    case pocketCard = "pocket_card"
    case deeplink
}

/// The moment the user asked for a page, read where they asked, carried to the page it opens.
public struct HostOpenTap: Sendable, Equatable {
    public let source: HostOpenSource
    public let openedAtEpochMilliseconds: Int64
    public let monotonicMilliseconds: Int

    public init(
        source: HostOpenSource,
        openedAtEpochMilliseconds: Int64,
        monotonicMilliseconds: Int
    ) {
        self.source = source
        self.openedAtEpochMilliseconds = openedAtEpochMilliseconds
        self.monotonicMilliseconds = monotonicMilliseconds
    }
}

/// Why a load step ran.
public enum HostLoadTrigger: String, Sendable {
    case open
    case refresh
    case prewarm
}

public extension HostEvent {
    /// A timed step: `outcome` and the session are tags, which Sentry can search, and the session stays an
    /// extra too. Steps also carry the OS version and the launch id they join `host.process.started` on.
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
        let session = hostSessionID?.telemetryValue

        var stepTags = tags
        stepTags["outcome"] = outcome.rawValue
        stepTags["host.session_id"] = session
        stepTags["host.os_version"] = origin.osVersion

        var stepExtras = extras
        stepExtras["duration_ms"] = .integer(durationMilliseconds)
        stepExtras["host.session_id"] = session.map(HostEventValue.string)
        stepExtras["app.launch_id"] = .string(origin.launchID.telemetryValue)

        let event = make(
            name: name,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: stepTags,
            extras: stepExtras
        )
        return HostEvent(
            name: event.name,
            tags: event.tags,
            extras: event.extras,
            durationMilliseconds: durationMilliseconds
        )
    }
}

private extension UUID {
    var telemetryValue: String { uuidString.lowercased() }
}
