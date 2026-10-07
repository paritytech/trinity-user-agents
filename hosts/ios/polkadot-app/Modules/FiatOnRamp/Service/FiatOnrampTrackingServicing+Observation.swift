import AsyncExtensions
import Foundation

extension FiatOnrampTrackingServicing {
    // MARK: Observation Tasks

    func startUpdateTriggersTask() {
        let logger = logger

        // Subscribed before triggers start: a late replay-subject consumer drops sends between replay and register.
        let updateEvents = updateTriggerEventSequence.makeAsyncIterator()

        updateTriggersTask = Task { [weak self] in
            await self?.emitTransactionStatusesUpdates()

            var iterator = updateEvents

            while let updateEvent = await iterator.next() {
                guard !Task.isCancelled, let self else {
                    return
                }

                do {
                    try await handleTriggerEvent(updateEvent)
                } catch is CancellationError {
                    return
                } catch {
                    logger.error("Fiat on-ramp tracking event failed: \(error)")
                }
            }
        }
    }

    func startSessionTransactionDiscoveryTask() {
        sessionDiscoveryTask = startPeriodicTriggerTask(
            taskName: "Fiat on-ramp discovery task",
            event: .discoverTransactions
        )
    }

    func startPollTransactionStatusesTask() {
        transactionPollingTask = startPeriodicTriggerTask(
            taskName: "Fiat on-ramp polling task",
            event: .pollTransactions
        )
    }

    func startDepositAutoSwapTask() {
        let logger = logger
        let clock = clock

        autoSwapDepositsTask = Task { [weak self] in
            await Self.runRetryingStreamLoop(
                clock: clock,
                logger: logger,
                streamName: "Fiat on-ramp auto swap stream"
            ) { [weak self] in
                guard let depositExecutionsStream = await self?.depositService.executions() else {
                    throw CancellationError()
                }

                for try await depositExecutions in depositExecutionsStream {
                    try Task.checkCancellation()
                    let fundedAssetExecutions = depositExecutions.filter {
                        $0.execLabel.chainAssetId == AppConfig.Assets.fiatOnrampFundedAsset
                    }

                    guard !fundedAssetExecutions.isEmpty else {
                        continue
                    }

                    self?.updateTriggerEventSequence.send(.autoSwap(fundedAssetExecutions))
                }
            }
        }
    }

    /// Retries a throwing async stream-consumer loop after non-cancellation failures.
    /// This is recovery/supervision logic, not polling cadence.
    private static func runRetryingStreamLoop(
        clock: any Clock<Duration>,
        logger: LoggerProtocol,
        streamName: String,
        operation: @escaping () async throws -> Void
    ) async {
        while !Task.isCancelled {
            do {
                try await operation()
            } catch is CancellationError {
                return
            } catch {
                logger.error("\(streamName) failed: \(error)")
            }

            guard !Task.isCancelled else {
                return
            }

            try? await clock.sleep(for: Timing.streamRetryDelay)
        }
    }

    private func startPeriodicTriggerTask(
        taskName: String,
        event: TriggerEvent
    ) -> Task<Void, Never> {
        let triggerEventSequence = updateTriggerEventSequence
        let clock = clock
        let logger = logger

        return Task {
            do {
                while !Task.isCancelled {
                    triggerEventSequence.send(event)
                    try await clock.sleep(for: Timing.pollingInterval)
                }
            } catch is CancellationError {
                return
            } catch {
                logger.error("\(taskName) failed: \(error)")
            }
        }
    }
}
