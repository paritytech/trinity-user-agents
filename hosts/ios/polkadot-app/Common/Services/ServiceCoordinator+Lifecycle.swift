import Foundation

protocol DashboardServicesCoordinating: AnyObject {
    @MainActor func startDurability() async -> Bool

    @MainActor func startInteractive()

    /// Leaves the durable engine running for an ongoing background run.
    @MainActor func stopInteractive()

    @MainActor func stopAll()
}

extension ServiceCoordinator {
    @MainActor
    func startDurability() async -> Bool {
        if let durabilityStart {
            return await durabilityStart.value
        }

        let start = Task { await runDurabilityStart() }
        durabilityStart = start
        return await start.value
    }

    @MainActor
    func startInteractive() {
        guard interactiveStart == nil else { return }

        // Keep the cached TLD warm on each launch without blocking.
        tldProvider.refresh()

        chatCoordinator.setup()
        productWorkerFacade.setup()
        chatExtensionsRegistry.discover()
        chatRequestCoordinator.setup()
        fiatOnrampTrackingService.setup()
        attachmentUploadService.setup()
        attachmentDownloadService.setup()
        notificationBadgeSyncService.setup()
        messageExpansionService.start()
        statementRenewalLoop.setup()

        interactiveStart = Task { [self] in
            await chainStatusProvider.start()
            await signInHostCoordinator.setup()
            await setupDeviceSyncService()

            guard await startDurability() else { return }

            // Recovering backup 1st
            await coinageTransferMonitor.setup()
            await w3sPaymentTracking.setup()
            await depositService.setup()
        }
    }

    @MainActor
    func stopInteractive() {
        guard let interactiveStart else { return }

        interactiveStart.cancel()
        self.interactiveStart = nil

        chatCoordinator.throttle()
        chatRequestCoordinator.throttle()
        fiatOnrampTrackingService.throttle()
        attachmentUploadService.throttle()
        attachmentDownloadService.throttle()
        notificationBadgeSyncService.throttle()
        statementRenewalLoop.throttle()

        messageExpansionService.stop()

        Task {
            await deviceSyncService.throttle()
            await coinageTransferMonitor.throttle()
            await w3sPaymentTracking.throttle()
            await signInHostCoordinator.throttle()
            await depositService.throttle()
        }
    }

    @MainActor
    func stopAll() {
        stopInteractive()
        durableTransactionEngine.txService.stop()

        // The workers and the product behind an opened card belong to this
        // session's runtime provider, so they go with the coordinator. Here
        // rather than in stopInteractive, because the Pocket is started once
        // when the coordinator is built and a coordinator kept alive by a
        // background run is started again when the dashboard returns.
        pocket?.stop()
    }
}

private extension ServiceCoordinator {
    func runDurabilityStart() async -> Bool {
        let chainRegistry = ChainRegistryFacade.sharedRegistry
        let mainAssetId = AppConfig.Assets.mainAsset

        guard
            let chain = chainRegistry.getChain(for: mainAssetId.chainId),
            let asset = chain.asset(for: mainAssetId.assetId)
        else {
            assertionFailure()
            return false
        }

        do {
            try await coinageService.setup(with: asset)
        } catch {
            logger.error("Coinage service setup failed: \(error)")
            return false
        }

        // After coinage setup, which releases uncommitted handoffs the first pass must not see.
        durableTransactionEngine.txService.start()
        return true
    }
}
