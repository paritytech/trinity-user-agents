import struct TrUAPIHost.ProductId
import Foundation
import IssueMonitoring
import os

final class WorkerBootTrace: Sendable {
    enum Phase {
        case resolve
        case executionOpen
    }

    let hostSessionId: UUID

    private let productId: ProductId
    private let telemetry: HostLoadTelemetry
    private let startedAt: Int
    private let phases = OSAllocatedUnfairLock(initialState: (
        timed: HostWorkerBootPhases(),
        webViewCreatedAt: Int?.none
    ))

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

    /// The resolve runs once; opening the execution has two halves, the builder's and the bridge's.
    func add(_ phase: Phase, since phaseStartedAt: Int?) {
        guard let phaseStartedAt, let now = telemetry.now() else { return }

        let elapsed = now - phaseStartedAt
        phases.withLock { phases in
            switch phase {
            case .resolve: phases.timed.resolve = phases.timed.resolve ?? elapsed
            case .executionOpen: phases.timed.executionOpen = (phases.timed.executionOpen ?? 0) + elapsed
            }
        }
    }

    /// Until the web view object exists, as Android times it; only the first one built counts.
    func markWebViewCreated(since createStartedAt: Int?) {
        guard let createStartedAt, let now = telemetry.now() else { return }

        phases.withLock { phases in
            guard phases.timed.webViewCreate == nil else { return }

            phases.timed.webViewCreate = now - createStartedAt
            phases.webViewCreatedAt = now
        }
    }

    /// From the web view existing until its blank page has loaded; only the first time.
    func markBlankReady() {
        guard let now = telemetry.now() else { return }

        phases.withLock { phases in
            guard phases.timed.blankReady == nil, let createdAt = phases.webViewCreatedAt else { return }

            phases.timed.blankReady = now - createdAt
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
        let phases = phases.withLock { $0.timed }

        telemetry.record { origin, now in
            HostLoadEvent.workerBoot(
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
