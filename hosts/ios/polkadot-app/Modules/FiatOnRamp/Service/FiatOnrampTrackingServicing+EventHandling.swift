import Foundation

extension FiatOnrampTrackingServicing {
    // MARK: Event Handling

    func handleTriggerEvent(_ event: TriggerEvent) async throws {
        switch event {
        case let .trackSession(id):
            await fiatOnrampStorage.addSessionId(
                id,
                createdAt: dateBuilder().timeIntervalSince1970
            )
        case .pollTransactions:
            await pollTransactionStatuses()
        case let .autoSwap(executions):
            try await handleAutoSwapExecutions(executions)
        case .discoverTransactions:
            try await discoverTransactions()
        case let .discoverTransactionsForSession(id):
            try await discoverTransactions(for: [id])
        case .removeFailedTransactions:
            await removeFailedTransactionsFromStorage()
        case .removeCompletedTransactions:
            await removeCompletedTransactionsFromStorage()
        }
    }

    func emitTransactionStatusesUpdates() async {
        let storedTransactions = await fiatOnrampStorage.getTrackedTransactions()
        let statuses = storedTransactions.map { transaction in
            FiatOnrampTransactionStatusPayload(
                id: transaction.id,
                status: .init(trackedTransactionStatus: transaction.status)
            )
        }

        transactionStatuses.send(Set(statuses))
    }

    private func removeTrackedTransactions(with ids: Set<FiatOnRampTransactionId>) async {
        guard !ids.isEmpty else {
            return
        }

        await fiatOnrampStorage.removeTrackedTransactions(ids)
        await emitTransactionStatusesUpdates()
    }

    private func removeFailedTransactionsFromStorage() async {
        let trackedTransactions = await fiatOnrampStorage.getTrackedTransactions()
        let failedIds = Set(trackedTransactions.compactMap { transaction in
            switch transaction.status {
            case .funding(.failed):
                return transaction.id
            case let .swapping(swapping):
                if case .failed = swapping.status {
                    return transaction.id
                }
                return nil
            default:
                return nil
            }
        })

        await removeTrackedTransactions(with: failedIds)
    }

    private func removeCompletedTransactionsFromStorage() async {
        let trackedTransactions = await fiatOnrampStorage.getTrackedTransactions()
        let completedIds = Set<FiatOnRampTransactionId>(trackedTransactions.compactMap { transaction in
            guard case let .swapping(swapping) = transaction.status else {
                return nil
            }

            if case .completed = swapping.status {
                return transaction.id
            }

            return nil
        })

        await removeTrackedTransactions(with: completedIds)
    }
}
