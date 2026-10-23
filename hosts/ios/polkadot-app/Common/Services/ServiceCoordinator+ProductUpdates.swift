import Foundation
import FoundationExt
import Products

extension ServiceCoordinator {
    static func registerProductUpdates(contentLoader: ProductContentLoading, logger: LoggerProtocol) {
        MainActor.assumeIsolated {
            let registered: ProductPresenceRegistering? = RootDependencyLocator.getDependency()

            guard registered == nil else { return }

            let coordinator = ProductUpdateCoordinator(
                contentLoader: contentLoader,
                workerReloader: UnwiredProductWorkerReloader(logger: logger),
                noticePresenter: ProductUpdateNoticePresenter(application: .shared),
                applicationStateStreams: ApplicationStateStreamFactory(),
                logger: logger
            )

            RootDependencyLocator.setDependency(coordinator as ProductPresenceRegistering)
            coordinator.start()
        }
    }
}
