import Foundation
import FoundationExt
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
                applicationStateStreams: ApplicationStateStreamFactory(),
                logger: logger
            )

            RootDependencyLocator.setDependency(coordinator as ProductPresenceRegistering)
            coordinator.start()
        }
    }
}
