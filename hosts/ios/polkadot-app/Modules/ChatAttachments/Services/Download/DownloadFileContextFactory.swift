import Foundation
import HandoffService
import Operation_iOS

final class DownloadFileContextFactory {
    let attachmentsStore: AttachmentStoring
    let repository: AnyDataProviderRepository<MixnetDownload>
    let chunkIndexRepository: AnyDataProviderRepository<MixnetDownloadChunkIndex>

    init(
        attachmentsStore: AttachmentStoring,
        repository: AnyDataProviderRepository<MixnetDownload>,
        chunkIndexRepository: AnyDataProviderRepository<MixnetDownloadChunkIndex>
    ) {
        self.attachmentsStore = attachmentsStore
        self.repository = repository
        self.chunkIndexRepository = chunkIndexRepository
    }

    func createContext(
        entryHash: FileHash,
        filename: String
    ) -> DownloadFileContext {
        DownloadFileContext(
            entryHash: entryHash,
            filename: filename,
            attachmentsStore: attachmentsStore,
            repository: repository,
            chunkIndexRepository: chunkIndexRepository
        )
    }
}

extension DownloadFileContextFactory {
    static func create(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared
    ) -> DownloadFileContextFactory? {
        guard let attachmentsStore = AttachmentStore.dowloads() else {
            return nil
        }

        let factory = MixnetDownloadRepositoryFactory(storageFacade: storageFacade)

        return DownloadFileContextFactory(
            attachmentsStore: attachmentsStore,
            repository: factory.createRepository(),
            chunkIndexRepository: factory.createChunkIndexRepository()
        )
    }
}
