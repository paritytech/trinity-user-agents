import Foundation
import Individuality
import IssueMonitoring
import MessageExchangeKit
import StatementDelivery
import StatementStore

/// A notification-funded statement toward a session's peer: a chat request or a chat acceptance.
protocol ChatNotificationStatementInput: OutgoingStatementInput {
    var session: MessageExchange.SessionRequest { get }
}

/// Picks signers for notification-funded statements: the period's one-shot account when a slot can be claimed,
/// otherwise the username account, which links the statement to us, only after the user consents.
final class ChatNotificationSignerResolver<Input: ChatNotificationStatementInput>: @unchecked Sendable {
    private let allocator: NotificationStatementAccountAllocating
    private let signerManager: StatementStoreSignerManaging
    private let fallback: NonPrivateSigningFallbacking
    private let renewalStarved: IssueDiagnostic?
    private let logger: LoggerProtocol

    init(
        allocator: NotificationStatementAccountAllocating,
        signerManager: StatementStoreSignerManaging,
        fallback: NonPrivateSigningFallbacking,
        renewalStarved: IssueDiagnostic?,
        logger: LoggerProtocol
    ) {
        self.allocator = allocator
        self.signerManager = signerManager
        self.fallback = fallback
        self.renewalStarved = renewalStarved
        self.logger = logger
    }
}

extension ChatNotificationSignerResolver: StatementSignerResolving {
    func currentPeriod() async throws -> UInt32 {
        try await allocator.currentPeriod()
    }

    func maxStatementSize() async throws -> Int {
        try await allocator.maxStatementSize()
    }

    func resolve(
        _ inputs: [Input],
        period: UInt32
    ) async throws -> [String: Result<StatementStoreSigning, Error>] {
        // Per input, so one unusable request id fails only its own statement.
        let oneShots = inputs.map { input in
            (input, Result { try signerManager.makeSigner(for: input.oneShotKeyId(period: period)) })
        }
        let accountIds = oneShots.compactMap { try? $0.1.get().accountId }

        // One batch in the given order, so the newest statements win when slots run short.
        let claimed = try await Set(allocator.initiateAllocations(for: accountIds))

        let resolved = await withTaskGroup(of: (String, Result<StatementStoreSigning, Error>).self) { group in
            for (input, oneShotResult) in oneShots {
                group.addTask {
                    switch oneShotResult {
                    case let .success(oneShot):
                        let isClaimed = claimed.contains(oneShot.accountId)
                        return await (input.statementId, self.resolve(input, oneShot: oneShot, isClaimed: isClaimed))
                    case let .failure(error):
                        return (input.statementId, .failure(error))
                    }
                }
            }

            return await group.reduce(into: [String: Result<StatementStoreSigning, Error>]()) { $0[$1.0] = $1.1 }
        }

        reportStarvation(in: resolved, period: period)

        return resolved
    }
}

private extension ChatNotificationSignerResolver {
    func resolve(
        _ input: Input,
        oneShot: StatementStoreSigning,
        isClaimed: Bool
    ) async -> Result<StatementStoreSigning, Error> {
        guard isClaimed else { return await identitySigner(for: input) }

        do {
            try await allocator.awaitAllocated(oneShot.accountId, timeout: ChatNotificationSlots.allocationTimeout)
            return .success(oneShot)
        } catch NotificationAllocationError.noFreeSlotInPeriod {
            return await identitySigner(for: input)
        } catch {
            return .failure(error)
        }
    }

    func identitySigner(for input: Input) async -> Result<StatementStoreSigning, Error> {
        do {
            return try await .success(fallback.identitySigner(for: input.session))
        } catch {
            return .failure(error)
        }
    }

    // Statements left with neither a slot nor the username account miss this period's copy.
    func reportStarvation(in resolved: [String: Result<StatementStoreSigning, Error>], period: UInt32) {
        guard let renewalStarved else { return }

        let starved = resolved.values.filter { result in
            guard case let .failure(error) = result else { return false }
            return !(error is OutgoingStatementTerminalError)
        }.count

        guard starved > 0 else { return }

        logger.warning("\(starved) statement(s) without a signer in period \(period)")
        renewalStarved.recordFailure(
            for: "\(period)",
            error: nil,
            counters: ["skipped": starved, "period": Int(period)]
        )
    }
}
