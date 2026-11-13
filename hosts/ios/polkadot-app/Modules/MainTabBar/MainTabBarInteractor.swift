import UIKit
import Combine
import Foundation
import PushKit
import PolkadotUI
import os
import TrUAPIHost

final class MainTabBarInteractor {
    weak var presenter: MainTabBarInteractorOutputProtocol?

    private let servicesLease: DashboardServicesLease<ServiceCoordinator>
    private let chainStatusProvider: ChainStatusProviding

    private let userNotificationService: UserNotificationServicing
    private let logger: LoggerProtocol
    private let urlHandlingService: URLHandlingServiceProtocol
    private let deferredLinkHandler: DeferredLinkHandling
    private let extensionWidgetStreamProvider: ChatExtensionWidgetStreaming
    private let productGamePills: ProductGamePillProviding?
    private let browserCoordinator: SPABrowserCoordinating
    private let tabBarLabelsStore: any TabBarLabelsProviding
    private let fundingServiceProvider: FundingServiceProviding?
    private let fundingService = OSAllocatedUnfairLock<FundingServiceProtocol?>(initialState: nil)

    private var extensionWidgetSubscription: Task<Void, Never>?
    private var productGamePillSubscription: Task<Void, Never>?
    private var chainStatusSubscription: Task<Void, Never>?
    private var tabBarLabelsSubscription: Task<Void, Never>?
    private var paymentsSetupSubscription: Task<Void, Never>?
    private var fundingSubscription: Task<Void, Never>?
    private var restoredBalanceSubscription: Task<Void, Never>?

    init(
        servicesLease: DashboardServicesLease<ServiceCoordinator>,
        chainStatusProvider: ChainStatusProviding,
        userNotificationService: UserNotificationServicing,
        urlHandlingService: URLHandlingServiceProtocol,
        deferredLinkHandler: DeferredLinkHandling,
        browserCoordinator: SPABrowserCoordinating,
        productGamePills: ProductGamePillProviding?,
        fundingServiceProvider: FundingServiceProviding?,
        logger: LoggerProtocol = Logger.shared,
        extensionWidgetStreamProvider: ChatExtensionWidgetStreaming? = nil,
        tabBarLabelsStore: any TabBarLabelsProviding = TabBarLabelsStore.shared
    ) {
        self.servicesLease = servicesLease
        self.chainStatusProvider = chainStatusProvider
        self.userNotificationService = userNotificationService
        self.logger = logger
        self.urlHandlingService = urlHandlingService
        self.deferredLinkHandler = deferredLinkHandler
        self.browserCoordinator = browserCoordinator
        self.productGamePills = productGamePills
        self.fundingServiceProvider = fundingServiceProvider
        self.extensionWidgetStreamProvider = extensionWidgetStreamProvider ?? ChatExtensionWidgetStreamProvider(
            registry: servicesLease.coordinator.chatExtensionsRegistry,
            logger: logger
        )
        self.tabBarLabelsStore = tabBarLabelsStore
    }

    deinit {
        unsubscribeFromExtensionWidgets()
        productGamePillSubscription?.cancel()
        servicesLease.release()
        chainStatusSubscription?.cancel()
        tabBarLabelsSubscription?.cancel()
        paymentsSetupSubscription?.cancel()
        fundingSubscription?.cancel()
        restoredBalanceSubscription?.cancel()
    }
}

extension MainTabBarInteractor: MainTabBarInteractorInputProtocol {
    func setup() {
        requestNotificationsAuthorization()
        subscribeToExtensionWidgets()
        subscribeToProductGamePill()
        deferredLinkHandler.register(urlHandlingService)
        subscribeToSPATabs()
        subscribeToChainStatus()
        subscribeToTabBarLabels()
        subscribeToPaymentsSetup()
        subscribeToFunding()
        subscribeToRestoredBalance()
    }

    func openProviderFrame(_ request: FundingFrameRequest) {
        fundingService.withLock { $0 }?.openProviderFrame(request)
    }

    func answer(frame: FundingFrame.ID, with outcome: FundingFrameOutcome) {
        fundingService.withLock { $0 }?.answer(frame: frame, with: outcome)
    }
}

private extension MainTabBarInteractor {
    /// Waits for the funding service, which comes with the runtime, then
    /// hands on what the core asks of the app's screens for as long as the
    /// tab bar is up.
    func subscribeToFunding() {
        guard let fundingServiceProvider else { return }

        fundingSubscription = Task { [weak self, fundingServiceProvider, logger] in
            for await service in fundingServiceProvider.builtFundingService() {
                self?.fundingService.withLock { $0 = service }
                await self?.presenter?.didReceive(fundingService: service)
                do {
                    for try await event in service.events {
                        await self?.presenter?.didReceive(fundingEvent: event)
                    }
                } catch {
                    logger.error("Funding events stream failed: \(error)")
                }
            }
        }
    }

    func subscribeToPaymentsSetup() {
        let coinageService = servicesLease.coordinator.coinageService
        paymentsSetupSubscription = Task { [weak self, coinageService, logger] in
            do {
                for try await isWaiting in coinageService.subscribeAwaitingInstallationBackup() {
                    await self?.presenter?.didReceivePaymentsSetupWaiting(isWaiting)
                }
            } catch {
                logger.error("Payments setup stream failed: \(error)")
            }
        }
    }

    func subscribeToRestoredBalance() {
        let backupSyncService = servicesLease.coordinator.coinageBackupSyncService
        restoredBalanceSubscription = Task { [weak self, backupSyncService, logger] in
            do {
                for try await isWaiting in backupSyncService.showsRecoveredBalanceStream {
                    await self?.presenter?.didReceiveRestoredBalanceWaiting(isWaiting)
                }
            } catch {
                logger.error("Restored balance stream failed: \(error)")
            }
        }
    }

    func subscribeToChainStatus() {
        chainStatusSubscription = Task { [weak self, chainStatusProvider, logger] in
            do {
                let statusStream = chainStatusProvider.statusStream()

                for try await rows in statusStream {
                    await self?.handleChainStatusUpdate(rows)
                }
            } catch {
                logger.error("Chain status stream failed: \(error)")
            }
        }
    }

    func requestNotificationsAuthorization() {
        userNotificationService.requestNotificationsAuthorization(completion: nil)
    }

    func subscribeToExtensionWidgets() {
        guard extensionWidgetSubscription == nil else {
            return
        }

        extensionWidgetSubscription = Task { [weak self, extensionWidgetStreamProvider] in
            do {
                let widgetConfigurationStream = extensionWidgetStreamProvider.widgetConfigurationStream()

                guard !Task.isCancelled else {
                    return
                }

                extensionWidgetStreamProvider.setup()

                for try await widget in widgetConfigurationStream {
                    await self?.handleExtensionWidgetUpdate(widget)
                }
            } catch {
                self?.logger.error("Extension widget stream failed: \(error)")
            }
        }
    }

    func subscribeToProductGamePill() {
        guard let productGamePills, productGamePillSubscription == nil else {
            return
        }

        productGamePillSubscription = Task { @MainActor [weak self, productGamePills, logger] in
            do {
                let pillStream = productGamePills.pillStream()
                productGamePills.start()

                for try await pill in pillStream {
                    self?.presenter?.didReceiveProductGamePill(pill)
                }
            } catch {
                logger.error("Product game pill stream failed: \(error)")
            }
        }
    }

    func unsubscribeFromExtensionWidgets() {
        extensionWidgetSubscription?.cancel()
        extensionWidgetSubscription = nil
        extensionWidgetStreamProvider.throttle()
    }

    func subscribeToSPATabs() {
        MainActor.assumeIsolated {
            browserCoordinator.addObserver(self, sendOnSubscription: true)
        }
    }

    func subscribeToTabBarLabels() {
        tabBarLabelsSubscription = Task { [weak self, tabBarLabelsStore, logger] in
            do {
                for try await isEnabled in tabBarLabelsStore.stream() {
                    await self?.presenter?.didReceiveTabBarLabelsEnabled(isEnabled)
                }
            } catch {
                logger.error("Tab bar labels subscription error: \(error)")
            }
        }
    }
}

@MainActor
private extension MainTabBarInteractor {
    func handleExtensionWidgetUpdate(_ update: ChatExtensionWidgetUpdate) {
        if let configuration = update.configuration {
            presenter?.didReceiveWidget(
                configuration: configuration,
                for: update.extensionId
            )
        } else {
            presenter?.didRemoveWidget(for: update.extensionId)
        }
    }

    func requestPolkadotSignIn(with url: URL) {
        presenter?.didReceivePolkadotSignInRequest(with: url)
    }

    func handleChainStatusUpdate(_ rows: [ChainConnectionStatusViewModel]) {
        presenter?.didReceiveChainStatus(rows)
    }
}

extension MainTabBarInteractor: PolkadotSignInServiceOutputProtocol {
    func didReceiveSignInUrl(_ url: URL) {
        Task { [weak self] in
            await self?.requestPolkadotSignIn(with: url)
        }
    }
}

extension MainTabBarInteractor: SPATabsObserver {
    func didReceiveUpdatedTabs(_ tabs: [SPATab]) {
        presenter?.didReceiveSPATabs(tabs)
    }
}
