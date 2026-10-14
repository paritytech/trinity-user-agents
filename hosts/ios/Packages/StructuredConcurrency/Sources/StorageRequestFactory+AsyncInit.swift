import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery

public extension StorageRequestFactory {
    static func asyncInit(timeout: Int = 60) -> StorageRequestFactoryProtocol {
        StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: OperationManagerFacade.sharedDefaultQueue),
            timeout: timeout
        )
    }
}
