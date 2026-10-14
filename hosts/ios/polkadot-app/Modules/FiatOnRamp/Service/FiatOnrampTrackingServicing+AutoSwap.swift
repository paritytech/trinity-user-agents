import Foundation

extension FiatOnrampTrackingServicing {
    // MARK: AutoSwap

    func handleAutoSwapExecutions(_ executions: [DepositExecutionItem]) async throws {
        // Transaction statuses are updated at polling cadence; it might
        // very well happen that a transaction is settled during the interval, and for auto swap to be executed.
        // The transaction status is refreshed to check transactions that were settled and are
        // candidates to be updated with auto swap progress.
        let shouldRefreshTransactions: Bool = executions.contains { execution in
            switch execution.status {
            case .pendingSwap:
                // Only pending swap is considered, as the point of starting the swap.
                true
            case .inProgress:
                // If a transaction is settled during a deposit execution being in progress, it most likely will not be
                // picked up by the auto swap, so don't poll the status
                false
            default:
                false
            }
        }

        if shouldRefreshTransactions {
            _ = await syncTransactions()
        }

        for depositExecution in executions {
            await handleAutoSwapExecution(depositExecution)
        }
        await emitTransactionStatusesUpdates()
    }

    /// Updates tracked transactions based on the latest auto-swap execution state.
    private func handleAutoSwapExecution(_ execution: DepositExecutionItem) async {
        let allTrackedTransactions = await fiatOnrampStorage.getTrackedTransactions()
        let now = dateBuilder().timeIntervalSince1970
        let execLabel = execution.execLabel

        let predicate: (FiatOnrampTrackedTransaction) -> Bool
        let swapStatus: FiatOnrampTrackedTransactionStatus.Swapping.Status

        switch execution.status {
        case let .pendingSwap(totalExecutionTime):
            predicate = { $0.status == .funding(.completed) }
            swapStatus = .inProgress(remainingTime: totalExecutionTime)
        case let .inProgress(remainedTime):
            predicate = { self.isSwapInProgress($0, label: execLabel) }
            swapStatus = .inProgress(remainingTime: remainedTime)
        case .completed:
            predicate = { self.isSwapInProgress($0, label: execLabel) }
            swapStatus = .completed
        case .failed:
            predicate = { self.isSwapInProgress($0, label: execLabel) }
            swapStatus = .failed
        }

        let updatedTransactions = allTrackedTransactions
            .filter(predicate)
            .map { transaction in
                var transaction = transaction
                transaction.status = .swapping(
                    .init(
                        status: swapStatus,
                        swapLabel: execLabel,
                        amountIn: execution.amountIn,
                        amountOut: execution.amountOut
                    )
                )
                transaction.lastUpdate = now
                return transaction
            }

        guard !updatedTransactions.isEmpty else {
            return
        }

        await fiatOnrampStorage.addTrackedTransactions(Set(updatedTransactions))
    }

    private func isSwapInProgress(
        _ transaction: FiatOnrampTrackedTransaction,
        label: DepositExecLabel
    ) -> Bool {
        guard case let .swapping(swap) = transaction.status else {
            return false
        }
        guard swap.swapLabel == label else {
            return false
        }
        if case .inProgress = swap.status {
            return true
        }
        return false
    }
}
