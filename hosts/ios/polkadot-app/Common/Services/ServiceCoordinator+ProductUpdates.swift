import Foundation
import FoundationExt
import Keystore_iOS
import Products

extension ServiceCoordinator {
    static func registerProductUpdates(
        contentLoader: ProductContentLoading,
        workerReloader: ProductWorkerReloading,
        iconFactory: ProductIconViewModelMaking,
        logger: LoggerProtocol
    ) {
        MainActor.assumeIsolated {
            let registered: ProductPresenceRegistering? = RootDependencyLocator.getDependency()

            guard registered == nil else { return }

            let coordinator = ProductUpdateCoordinator(
                contentLoader: contentLoader,
                workerReloader: workerReloader,
                noticePresenter: ProductUpdateNoticePresenter(application: .shared, iconFactory: iconFactory),
                refreshSources: ProductUpdateCoordinator.RefreshSources(
                    applicationStateStreams: ApplicationStateStreamFactory(),
                    hostPlacedProducts: HostPlacedProductsProvider(
                        settingsManager: SettingsManager.shared,
                        tldProvider: DotNsTldProviderFacade.shared
                    )
                ),
                logger: logger
            )

            RootDependencyLocator.setDependency(coordinator as ProductPresenceRegistering)
            coordinator.start()
        }
    }
}
