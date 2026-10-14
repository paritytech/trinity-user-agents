import Foundation
import Operation_iOS
import SubstrateMetadataHash
import SubstrateSdk
import ChainRegistry

final class RuntimeMetadataItemProvider {
    let runtimeMetadataRepositoryFactory: RuntimeMetadataRepositoryFactoryProtocol

    init(runtimeMetadataRepositoryFactory: RuntimeMetadataRepositoryFactoryProtocol) {
        self.runtimeMetadataRepositoryFactory = runtimeMetadataRepositoryFactory
    }
}

extension RuntimeMetadataItemProvider: RuntimeMetadataItemProviding {
    func createFetchWrapper(for chainId: ChainId) -> CompoundOperationWrapper<RuntimeMetadataItemProtocol?> {
        let repository = runtimeMetadataRepositoryFactory.createRepository(for: chainId)

        let fetchOperation = repository.fetchOperation(by: { chainId }, options: RepositoryFetchOptions())

        let mapOperation = ClosureOperation<RuntimeMetadataItemProtocol?> {
            try fetchOperation.extractNoCancellableResultData()
        }

        mapOperation.addDependency(fetchOperation)

        return CompoundOperationWrapper(targetOperation: mapOperation, dependencies: [fetchOperation])
    }
}

extension RuntimeMetadataItem: RuntimeMetadataItemProtocol {}
