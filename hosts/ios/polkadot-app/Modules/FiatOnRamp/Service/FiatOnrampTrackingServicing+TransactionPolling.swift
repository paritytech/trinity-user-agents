import Foundation

extension FiatOnrampTrackingServicing {
    // MARK: Transaction Polling

    func pollTransactionStatuses() async {
        let transactions = await syncTransactions()
        guard !transactions.isEmpty else {
            return
        }
        await emitTransactionStatusesUpdates()
    }

    func syncTransactions() async -> [FiatOnrampTransactionSummary] {
        let transactions = await fetchTransactions()
        await processPolledTransactions(transactions)
        return transactions
    }

    private func fetchTransactions() async -> [FiatOnrampTransactionSummary] {
        let trackedTransactions = await fiatOnrampStorage.getTrackedTransactions()
        let pendingTrackedTransactions = trackedTransactions.filter {
            switch $0.status {
            case .funding(.inProgress):
                true
            default:
                false
            }
        }

        return await withTaskGroup(of: FiatOnrampTransactionSummary?.self) { [weak self] group in
            for trackedTransaction in pendingTrackedTransactions {
                group.addTask {
                    guard let self else {
                        return nil
                    }

                    do {
                        return try await self.fiatOnrampService.fetchTransaction(id: trackedTransaction.id)
                    } catch {
                        self.logger.error(
                            "Fiat on-ramp transaction fetch failed for \(trackedTransaction.id.value): \(error)"
                        )
                        return nil
                    }
                }
            }

            var transactionResponses: [FiatOnrampTransactionSummary] = []

            for await transactionResponse in group {
                if let transactionResponse {
                    transactionResponses.append(transactionResponse)
                }
            }

            return transactionResponses
        }
    }

    func discoverTransactions() async throws {
        let now = dateBuilder().timeIntervalSince1970
        let expiredSessionIds = await fiatOnrampStorage.removeExpiredSessionIds(
            olderThan: now - Timing.sessionDiscoveryTtl
        )
        if !expiredSessionIds.isEmpty {
            logger.warning(
                "Fiat on-ramp discovery dropped \(expiredSessionIds.count) expired pending sessions."
            )
        }

        let sessionIds = await fiatOnrampStorage.getSessionIds()
        guard !sessionIds.isEmpty else {
            return
        }
        try await discoverTransactions(for: sessionIds)
    }

    func discoverTransactions(for sessionIds: Set<FiatOnRampSessionId>) async throws {
        var transactions = try await fiatOnrampService.fetchTransactions(.init(sessionIds: sessionIds))
        let trackedTransactions = await fiatOnrampStorage.getTrackedTransactions()
        transactions = transactions.filter { transaction in
            !trackedTransactions.contains { $0.id == transaction.transactionId }
        }

        guard !transactions.isEmpty else {
            return
        }
        await processPolledTransactions(transactions)
        await emitTransactionStatusesUpdates()
    }

    private func processPolledTransactions(_ transactions: [FiatOnrampTransactionSummary]) async {
        let sessionIds = transactions.map(\.sessionId)
        let now = dateBuilder().timeIntervalSince1970
        let updatedTransactions = transactions.map { summary in
            FiatOnrampTrackedTransaction(
                id: summary.transactionId,
                status: .funding(.init(fundingTransactionStatus: summary.status)),
                lastUpdate: now
            )
        }
        await fiatOnrampStorage.addTrackedTransactions(Set(updatedTransactions))
        await fiatOnrampStorage.removeSessionIds(Set(sessionIds))
    }
}
