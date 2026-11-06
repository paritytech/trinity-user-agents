import Foundation
import IssueMonitoring

enum HostLoadEvent {
    static func rpcConnected(
        chain: String,
        executable: HostExecutableOwner?,
        outcome: HostEventOutcome,
        durationMilliseconds: Int,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        var tags = ["rpc.chain": chain]
        tags["product.id"] = executable?.productID
        tags["executable.kind"] = executable?.kind.rawValue

        return .step(
            name: "host.rpc.connected",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: executable?.hostSessionID,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: tags
        )
    }

    // swiftlint:disable:next function_parameter_count
    static func pocketFaceLoaded(
        productID: String,
        outcome: HostEventOutcome,
        worker: HostWorkerState,
        durationMilliseconds: Int,
        hostSessionID: UUID?,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        .step(
            name: "host.pocket.face_loaded",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: hostSessionID,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: ["product.id": productID, "worker.state": worker.rawValue]
        )
    }

    // swiftlint:disable:next function_parameter_count
    static func workerBoot(
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

        return .step(
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

    static func runtimeBoot(
        outcome: HostEventOutcome,
        durationMilliseconds: Int,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        .step(
            name: "host.runtime.boot",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: nil,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: [:]
        )
    }

    // swiftlint:disable:next function_parameter_count
    static func webViewPageLoaded(
        hostSessionID: UUID,
        productID: String,
        outcome: HostEventOutcome,
        durationMilliseconds: Int,
        tapMilliseconds: Int?,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        .step(
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

    // swiftlint:disable:next function_parameter_count
    static func pocketCardTapped(
        productID: String,
        outcome: HostEventOutcome,
        source: HostOpenSource,
        durationMilliseconds: Int,
        hostSessionID: UUID?,
        origin: HostEventOrigin,
        monotonicMilliseconds: Int
    ) -> HostEvent {
        .step(
            name: "host.pocket.card_tapped",
            outcome: outcome,
            durationMilliseconds: durationMilliseconds,
            hostSessionID: hostSessionID,
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: ["product.id": productID, "open.source": source.rawValue],
            extras: outcome == .reused ? ["reuse_ms": .integer(durationMilliseconds)] : [:]
        )
    }
}

public enum HostExecutableKind: String, Sendable {
    case page
    case worker
}

enum HostWorkerState: String, Sendable {
    case warm
    case cold
}

public struct HostExecutableOwner: Sendable, Equatable {
    let productID: String
    let kind: HostExecutableKind
    let hostSessionID: UUID?
}

struct HostWorkerBootPhases: Sendable, Equatable {
    var resolve: Int?
    var executionOpen: Int?
    var webViewCreate: Int?
    var blankReady: Int?
}
