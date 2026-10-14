import UIKit
import Combine
import Keystore_iOS

@MainActor
protocol TabFactoryProtocol {
    func view(for item: TabBarItem) -> UIViewController?
    #if FEATURE_INPUT
        func makeScanController() -> ScanPanelViewController?
    #else
        func makeScanController() -> ScanPanelPlainViewController?
    #endif
}

final class TabFactory: TabFactoryProtocol {
    private let serviceCoordinator: ServiceCoordinatorProtocol
    private let flowState: ChatFlowState
    private weak var foregroundVisibilityReporter: PushForegroundVisibilityReporting?
    private let scanResultHandler: WalletQRScanDelegate
    private let flowStateProvider: any SPAFlowStateProviding

    init(
        serviceCoordinator: ServiceCoordinatorProtocol,
        flowState: ChatFlowState,
        scanResultHandler: WalletQRScanDelegate,
        flowStateProvider: any SPAFlowStateProviding,
        foregroundVisibilityReporter: PushForegroundVisibilityReporting? = nil
    ) {
        self.serviceCoordinator = serviceCoordinator
        self.flowState = flowState
        self.scanResultHandler = scanResultHandler
        self.flowStateProvider = flowStateProvider
        self.foregroundVisibilityReporter = foregroundVisibilityReporter
    }

    func view(for item: TabBarItem) -> UIViewController? {
        let mainContentVC: UIViewController? =
            switch item {
            case .chat:
                createChatTab()
            case .wallet:
                createWalletTab()
            case .browse:
                createBrowseTab()
            case .settings:
                createSettingsTab()
            }

        return mainContentVC
    }

    #if FEATURE_INPUT
        func makeScanController() -> ScanPanelViewController? {
            let scannerView = WalletQRScanViewFactory.createView(for: scanResultHandler, presentation: .embedded)

            guard let scanner = scannerView?.controller as? (UIViewController & ScanPanelScannerControlling) else {
                return nil
            }

            guard let search = SearchContactModuleFactory.makeModule() else {
                return nil
            }

            let controller = ScanPanelViewController(scannerController: scanner, presenter: search.presenter)
            search.presenter.view = controller
            search.wireframe.onChatFound = { [weak controller] model in
                controller?.onChatFound?(model)
            }

            return controller
        }
    #else
        func makeScanController() -> ScanPanelPlainViewController? {
            guard let scanner = WalletQRScanViewFactory.createView(
                for: scanResultHandler,
                presentation: .embedded
            )?.controller else {
                return nil
            }

            return ScanPanelPlainViewController(scannerController: scanner)
        }
    #endif
}

// MARK: Tab content

private extension TabFactory {
    private func createChatTab() -> UIViewController? {
        guard let view = ContactsListViewFactory.createView(flowState: flowState) else {
            return nil
        }

        let navigation = AppNavigationController(rootViewController: view.controller)

        navigation.barSettings = .shadowSettings
        navigation.scrollEdgeBarSettings = .defaultSettings

        return navigation
    }

    private func createWalletTab() -> UIViewController? {
        let spaFlowState = flowStateProvider.flowState()
        let context = WalletFlowContext(
            depositService: serviceCoordinator.depositService,
            fiatOnrampService: serviceCoordinator.fiatOnrampService,
            fiatOnrampTrackingService: serviceCoordinator.fiatOnrampTrackingService,
            coinageService: serviceCoordinator.coinageService,
            coinageBackupSyncService: serviceCoordinator.coinageBackupSyncService,
            networkStatusService: serviceCoordinator.networkStatusService,
            flowState: spaFlowState
        )
        guard let view = WalletMainViewFactory.createView(
            with: context,
            chainAssetId: AppConfig.Assets.mainAsset
        )
        else {
            return nil
        }

        let navigation = AppNavigationController(rootViewController: view.controller)

        return navigation
    }

    private func createBrowseTab() -> UIViewController? {
        let view = BrowseViewFactory.createView(
            flowStateProvider: flowStateProvider
        )

        let navigation = AppNavigationController(rootViewController: view.controller)

        navigation.barSettings = .shadowSettings
        navigation.scrollEdgeBarSettings = .defaultSettings

        return navigation
    }

    private func createSettingsTab() -> UIViewController? {
        guard let view = SettingsViewFactory.createView(
            serviceCoordinator: serviceCoordinator,
            flowStateProvider: flowStateProvider
        ) else {
            return nil
        }

        let navigation = AppNavigationController(rootViewController: view.controller)

        navigation.barSettings = .shadowSettings
        navigation.scrollEdgeBarSettings = .defaultSettings

        return navigation
    }
}
