import struct TrUAPIHost.ProductId
import Foundation
import IssueMonitoring
import os

final class WorkerBootTrace: Sendable {
    enum Phase {
        case resolve
        case executionOpen
        case webViewCreate
        case blankReady
    }

    let hostSessionId: UUID

    private let productId: ProductId
    private let telemetry: HostLoadTelemetry
    private let startedAt: Int
    private let phases = OSAllocatedUnfairLock(initialState: HostWorkerBootPhases())

    init?(productId: ProductId, telemetry: HostLoadTelemetry) {
        guard let hostSessionId = telemetry.makeSessionID(), let startedAt = telemetry.now() else { return nil }

        self.productId = productId
        self.telemetry = telemetry
        self.hostSessionId = hostSessionId
        self.startedAt = startedAt
    }

    func now() -> Int? {
        telemetry.now()
    }

    func add(_ phase: Phase, since phaseStartedAt: Int?) {
        guard let phaseStartedAt, let now = telemetry.now() else { return }

        let elapsed = now - phaseStartedAt
        phases.withLock { phases in
            switch phase {
            case .resolve: phases.resolve = (phases.resolve ?? 0) + elapsed
            case .executionOpen: phases.executionOpen = (phases.executionOpen ?? 0) + elapsed
            case .webViewCreate: phases.webViewCreate = (phases.webViewCreate ?? 0) + elapsed
            case .blankReady: phases.blankReady = (phases.blankReady ?? 0) + elapsed
            }
        }
    }

    func withSession<R>(
        isolation: isolated (any Actor)? = #isolation,
        operation: () async throws -> R
    ) async rethrows -> R {
        try await telemetry.withContext(
            session: hostSessionId,
            trigger: .open,
            isolation: isolation,
            operation: operation
        )
    }

    func finish(_ outcome: HostEventOutcome) {
        let phases = phases.withLock { $0 }

        telemetry.record { origin, now in
            .workerBoot(
                productID: productId.rawValue,
                outcome: outcome,
                phases: phases,
                durationMilliseconds: now - startedAt,
                hostSessionID: hostSessionId,
                origin: origin,
                monotonicMilliseconds: now
            )
        }
    }
}
