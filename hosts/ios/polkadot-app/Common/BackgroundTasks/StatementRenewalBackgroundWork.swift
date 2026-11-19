import BackgroundExecution
import Foundation
import StatementDelivery

struct StatementRenewalBackgroundWork: Sendable {
    func callAsFunction() async -> BackgroundTaskWorkOutcome {
        let manager = await DashboardServices.manager
        let interactiveRunning = await manager.isInteractiveRunning

        guard let lease = await manager.acquireDurability() else {
            return .skipped(reason: "notReady interactiveRunning=\(interactiveRunning)")
        }

        // Not from the expiration handler: stopping durability must not race a claim still in flight.
        defer { lease.release() }

        let renewal = await lease.coordinator.durabilityRenewal.renewAll()

        return .completed(logFields: Self.logFields(interactiveRunning: interactiveRunning, renewal: renewal))
    }
}

private extension StatementRenewalBackgroundWork {
    static func logFields(interactiveRunning: Bool, renewal: StatementDurabilityRenewalReport) -> String {
        [
            "interactiveRunning=\(interactiveRunning)",
            fields(prefix: "requests", renewal.chatRequests),
            fields(prefix: "acceptances", renewal.chatAcceptances)
        ].joined(separator: " ")
    }

    static func fields(prefix: String, _ summary: OutgoingStatementRenewalSummary) -> String {
        guard summary.isComplete else { return "\(prefix)=incomplete" }

        return "\(prefix)Due=\(summary.due) \(prefix)Renewed=\(summary.republished) "
            + "\(prefix)Failed=\(summary.failed) \(prefix)MissedPeriods=\(summary.missedPeriods)"
    }
}
