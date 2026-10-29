import Foundation
import StructuredConcurrency
import StatementDelivery

protocol StatementDurabilityRenewing: Sendable {
    @discardableResult
    func renewAll() async -> StatementDurabilityRenewalReport
}

protocol AllowanceSlotsRenewing: Sendable {
    func renewAllowanceSlots() async
}

protocol StatementRenewerProviding: Sendable {
    func makeChatRequestRenewer() async throws -> OutgoingStatementRenewing
    func makeChatAcceptanceRenewer() async throws -> OutgoingStatementRenewing
}

struct StatementDurabilityRenewalReport: Sendable, Equatable {
    let chatRequests: OutgoingStatementRenewalSummary
    let chatAcceptances: OutgoingStatementRenewalSummary

    static let cancelled = StatementDurabilityRenewalReport(chatRequests: .incomplete, chatAcceptances: .incomplete)
}

/// Every trigger goes through here, so concurrent ones share one run instead of publishing twice.
actor StatementDurabilityRenewalService {
    private let allowances: AllowanceSlotsRenewing
    private let renewers: StatementRenewerProviding
    private let issues: StatementRenewalIssueDiagnostics
    private let sharedRun: any SharedRunning<StatementDurabilityRenewalReport>
    private let logger: LoggerProtocol

    init(
        allowances: AllowanceSlotsRenewing,
        renewers: StatementRenewerProviding,
        issues: StatementRenewalIssueDiagnostics,
        sharedRun: any SharedRunning<StatementDurabilityRenewalReport>,
        logger: LoggerProtocol
    ) {
        self.allowances = allowances
        self.renewers = renewers
        self.issues = issues
        self.sharedRun = sharedRun
        self.logger = logger
    }
}

extension StatementDurabilityRenewalService: StatementDurabilityRenewing {
    @discardableResult
    func renewAll() async -> StatementDurabilityRenewalReport {
        await sharedRun.run { [self] in
            await performRun()
        }
    }
}

private extension StatementDurabilityRenewalService {
    func performRun() async -> StatementDurabilityRenewalReport {
        await allowances.renewAllowanceSlots()

        guard !Task.isCancelled else { return .cancelled }

        async let chatRequests = renew(.chatRequests)
        async let chatAcceptances = renew(.chatAcceptances)

        let report = await StatementDurabilityRenewalReport(
            chatRequests: chatRequests,
            chatAcceptances: chatAcceptances
        )
        issues.record(report)
        return report
    }

    enum StatementKind {
        case chatRequests
        case chatAcceptances
    }

    func renew(_ kind: StatementKind) async -> OutgoingStatementRenewalSummary {
        do {
            let renewer =
                switch kind {
                case .chatRequests: try await renewers.makeChatRequestRenewer()
                case .chatAcceptances: try await renewers.makeChatAcceptanceRenewer()
                }

            return await renewer.renew()
        } catch {
            logger.error("Statement renewer unavailable: \(error)")
            return .incomplete
        }
    }
}
