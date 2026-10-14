import Foundation
import HandoffService
import Individuality
import SDKLogger
import ChainRegistry

extension ServiceCoordinator {
    static func createAttachmentUploadService(
        bulletInManager: AllowanceManaging
    ) -> AttachmentUploadingServicing? {
        let chainRegistry = ChainRegistryFacade.sharedRegistry
        let hopNodeProvider = HOPNodeProvider(chainRegistry: chainRegistry)

        guard
            let uploadContextFactory = UploadFileContextFactory.create(
                nodeProvider: hopNodeProvider
            )
        else {
            return nil
        }

        let loaderFactory = HOPFileLoaderFactory(logger: Logger.shared)

        return MixnetUploadService(
            loaderFactory: loaderFactory,
            storageFacade: UserDataStorageFacade.shared,
            uploadContextFactory: uploadContextFactory,
            senderProvider: AttachmentsSenderProvider(),
            allowanceManager: bulletInManager
        )
    }

    static func createAttachmentDownloadService() -> AttachmentDownloadingServicing? {
        guard
            let attachmentsStore = AttachmentStore.dowloads(),
            let downloadContextFactory = DownloadFileContextFactory.create() else {
            return nil
        }

        let loaderFactory = HOPFileLoaderFactory(logger: Logger.shared)
        let hopNodeProvider = HOPNodeProvider(chainRegistry: ChainRegistryFacade.sharedRegistry)

        return MixnetDownloadService(
            loaderFactory: loaderFactory,
            hopNodeProvider: hopNodeProvider,
            storageFacade: UserDataStorageFacade.shared,
            attachmentsStore: attachmentsStore,
            downloadContextFactory: downloadContextFactory
        )
    }
}
