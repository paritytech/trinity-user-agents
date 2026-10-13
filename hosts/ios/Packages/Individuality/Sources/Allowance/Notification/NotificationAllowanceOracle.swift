import DurableTransactions
import Foundation
import SDKLogger
import SubstrateSdk

/// Decides a claim by whether its target account holds a statement allowance.
///
/// Monotone and singly written: every claim targets a fresh account per period, nothing else grants that
/// account an allowance, and the allowance is revoked only a full grace window after its period.
public enum NotificationAllowanceOracle {
    public static func make(
        chainId: ChainId,
        allowanceRepository: StatementStoreAllowanceRepositoryProtocol,
        logger: SDKLoggerProtocol
    ) -> MonotoneEffectOracle {
        MonotoneEffectOracle(chainId: chainId) { transactions, block in
            let targetByClaim = transactions.reduce(into: [DurableTxId: AccountId]()) { result, transaction in
                result[transaction.id] = NotificationSlotDomain.target(of: transaction.groupId)
            }

            let granted: [AccountId: Bool]
            do {
                granted = try await allowanceRepository.hasAllowance(
                    Array(Set(targetByClaim.values)),
                    at: block.hash
                )
            } catch {
                logger.warning("Notification allowance read failed at block \(block.number): \(error)")
                return [:]
            }

            return targetByClaim.compactMapValues { granted[$0] }
        }
    }
}
