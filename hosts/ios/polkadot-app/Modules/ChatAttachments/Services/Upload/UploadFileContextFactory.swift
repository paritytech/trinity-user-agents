import Foundation
import HandoffService
import Operation_iOS

final class UploadFileContextFactory {
    let attachmentsStore: AttachmentStoring
    let nodeProvider: HOPNodeProviding
    let repository: AnyDataProviderRepository<MixnetUpload>
    let updateRepository: AnyDataProviderRepository<MixnetUploadUpdate>

    init(
        attachmentsStore: AttachmentStoring,
        nodeProvider: HOPNodeProviding,
        repository: AnyDataProviderRepository<MixnetUpload>,
        updateRepository: AnyDataProviderRepository<MixnetUploadUpdate>
    ) {
        self.attachmentsStore = attachmentsStore
        self.nodeProvider = nodeProvider
        self.repository = repository
        self.updateRepository = updateRepository
    }

    func createContext(attachmentId: AttachmentId) -> UploadFileContext? {
        let fileURL = attachmentsStore.fileURL(for: attachmentId.fileId)

        guard attachmentsStore.hasFile(for: attachmentId.fileId) else {
            return nil
        }

        let attributes = try? FileManager.default.attributesOfItem(atPath: fileURL.path)
        let fileSize = (attributes?[.size] as? Int) ?? 0

        return UploadFileContext(
            attachmentId: attachmentId.stringValue,
            fileURL: fileURL,
            fileSize: fileSize,
            nodeProvider: nodeProvider,
            repository: repository,
            updateRepository: updateRepository
        )
    }
}

extension UploadFileContextFactory {
    static func create(
        nodeProvider: HOPNodeProviding,
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared
    ) -> UploadFileContextFactory? {
        guard let attachmentsStore = AttachmentStore.uploads() else {
            return nil
        }

        let factory = MixnetUploadRepositoryFactory(storageFacade: storageFacade)

        return UploadFileContextFactory(
            attachmentsStore: attachmentsStore,
            nodeProvider: nodeProvider,
            repository: factory.createRepository(),
            updateRepository: factory.createUpdateRepository()
        )
    }
}
