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

    public var durationMilliseconds: Int? {
        guard case let .integer(duration)? = extras["duration_ms"] else { return nil }

        return duration
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
            tags: ["product.id": productID, "trigger": trigger.rawValue]
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
            tags: ["trigger": trigger.rawValue],
            extras: extras
        )
    }

    public static func rpcConnected(
        chain: String,
        executable: HostExecutableOwner?,
        outcome: HostEventOutcome,
        durationMilliseconds: Int,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        var tags = ["chain": chain]
        tags["product.id"] = executable?.productID
        tags["executable.kind"] = executable?.kind.rawValue

        return step(
            name: "host.rpc.connected",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: executable?.hostSessionID,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: tags
        )
    }

    public static func pocketFaceLoaded(
        productID: String,
        outcome: HostEventOutcome,
        worker: HostWorkerTemperature,
        durationMilliseconds: Int,
        hostSessionID: UUID?,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        step(
            name: "host.pocket.face_loaded",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: hostSessionID,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: ["product.id": productID, "worker": worker.rawValue]
        )
    }

    public static func workerBoot(
        productID: String,
        outcome: HostEventOutcome,
        phases: HostWorkerBootPhases,
        durationMilliseconds: Int,
        hostSessionID: UUID?,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        var extras: [String: HostEventValue] = [:]
        extras["resolve_ms"] = phases.resolve.map(HostEventValue.integer)
        extras["execution_open_ms"] = phases.executionOpen.map(HostEventValue.integer)
        extras["webview_create_ms"] = phases.webViewCreate.map(HostEventValue.integer)
        extras["blank_ready_ms"] = phases.blankReady.map(HostEventValue.integer)

        return step(
            name: "host.worker.boot",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: hostSessionID,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: ["product.id": productID],
            extras: extras
        )
    }

    public static func runtimeBoot(
        outcome: HostEventOutcome,
        durationMilliseconds: Int,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        step(
            name: "host.runtime.boot",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: nil,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: [:]
        )
    }

    public static func webViewPageLoaded(
        hostSessionID: UUID,
        productID: String,
        outcome: HostEventOutcome,
        durationMilliseconds: Int,
        tapMilliseconds: Int? = nil,
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
            tags: ["product.id": productID],
            extras: tapMilliseconds.map { ["tap_ms": .integer($0)] } ?? [:]
        )
    }

    /// The user tapping a Pocket card, or following a link to one: `opened` starts a page load,
    /// `reused` shows the page already held, `reuseMilliseconds` after the tap.
    public static func pocketCardTapped(
        productID: String,
        outcome: HostEventOutcome,
        source: HostOpenSource,
        reuseMilliseconds: Int?,
        hostSessionID: UUID?,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        let session = hostSessionID?.telemetryValue

        var tags = ["product.id": productID, "outcome": outcome.rawValue, "open.source": source.rawValue]
        tags["host.session_id"] = session

        var extras: [String: HostEventValue] = [:]
        extras["reuse_ms"] = reuseMilliseconds.map(HostEventValue.integer)
        extras["host.session_id"] = session.map(HostEventValue.string)

        return make(
            name: "host.pocket.card_tapped",
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: tags,
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
    case other
}

/// The moment the user asked for a page, read where they asked, carried to the page it opens.
public struct HostOpenTap: Sendable, Equatable {
    public let hostSessionID: UUID
    public let source: HostOpenSource
    public let openedAtEpochMilliseconds: Int64
    public let monotonicMilliseconds: Int

    public init(
        hostSessionID: UUID,
        source: HostOpenSource,
        openedAtEpochMilliseconds: Int64,
        monotonicMilliseconds: Int
    ) {
        self.hostSessionID = hostSessionID
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

public enum HostExecutableKind: String, Sendable {
    case page
    case worker
}

public enum HostWorkerTemperature: String, Sendable {
    case warm
    case cold
}

public struct HostExecutableOwner: Sendable, Equatable {
    public let productID: String
    public let kind: HostExecutableKind
    public let hostSessionID: UUID?

    public init(productID: String, kind: HostExecutableKind, hostSessionID: UUID?) {
        self.productID = productID
        self.kind = kind
        self.hostSessionID = hostSessionID
    }
}

public struct HostWorkerBootPhases: Sendable, Equatable {
    public var resolve: Int?
    public var executionOpen: Int?
    public var webViewCreate: Int?
    public var blankReady: Int?

    public init(resolve: Int? = nil, executionOpen: Int? = nil, webViewCreate: Int? = nil, blankReady: Int? = nil) {
        self.resolve = resolve
        self.executionOpen = executionOpen
        self.webViewCreate = webViewCreate
        self.blankReady = blankReady
    }
}

private extension UUID {
    var telemetryValue: String { uuidString.lowercased() }
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
        let session = hostSessionID?.telemetryValue

        var stepTags = tags
        stepTags["outcome"] = outcome.rawValue
        stepTags["host.session_id"] = session

        var stepExtras = extras
        stepExtras["duration_ms"] = .integer(durationMilliseconds)
        stepExtras["host.session_id"] = session.map(HostEventValue.string)

        return make(
            name: name,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: stepTags,
            extras: stepExtras
        )
    }
}
