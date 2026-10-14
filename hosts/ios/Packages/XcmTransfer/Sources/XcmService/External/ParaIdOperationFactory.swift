import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import ChainStore

public protocol ParaIdOperationFactoryProtocol {
    func createParaIdOperation(for chainId: ChainId) -> CompoundOperationWrapper<ParaId>
}

public enum ParaIdOperationFactoryError: Error {
    case undefinedParaId
}

public final class ParaIdOperationFactory: ParaIdOperationFactoryProtocol {
    let chainRegistry: ChainResourceProtocol

    private var cachedParaIds: [ChainId: ParaId] = [:]
    private var mutex = NSLock()

    let storageRequestFactory: StorageRequestFactoryProtocol

    public init(chainRegistry: ChainResourceProtocol, operationQueue: OperationQueue) {
        self.chainRegistry = chainRegistry

        let operationManager = OperationManager(operationQueue: operationQueue)
        storageRequestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: operationManager
        )
    }

    private func getParaId(for chainId: ChainId) -> ParaId? {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return cachedParaIds[chainId]
    }

    private func setParaId(_ paraId: ParaId?, for chainId: ChainId) {
        mutex.lock()

        cachedParaIds[chainId] = paraId

        mutex.unlock()
    }

    public func createParaIdOperation(for chainId: ChainId) -> CompoundOperationWrapper<ParaId> {
        if let paraId = getParaId(for: chainId) {
            return CompoundOperationWrapper.createWithResult(paraId)
        }

        guard let runtimeProvider = chainRegistry.getRuntimeCodingService(
            for: chainId
        ) else {
            return CompoundOperationWrapper.createWithError(
                ChainResourceError.runtimeMetadaUnavailable
            )
        }

        guard let connection = chainRegistry.getRpcConnection(for: chainId) else {
            return CompoundOperationWrapper.createWithError(
                ChainResourceError.connectionUnavailable
            )
        }

        let coderFactoryOperation = runtimeProvider.fetchCoderFactoryOperation()
        let wrapper: CompoundOperationWrapper<StorageResponse<StringScaleMapper<ParaId>>>

        wrapper = storageRequestFactory.queryItem(
            engine: connection,
            factory: { try coderFactoryOperation.extractNoCancellableResultData() },
            storagePath: ParachainInfoPallet.parachainId
        )

        wrapper.addDependency(operations: [coderFactoryOperation])

        let updateOperation = ClosureOperation<ParaId> { [weak self] in
            let response = try wrapper.targetOperation.extractNoCancellableResultData()

            guard let paraId = response.value?.value else {
                throw ParaIdOperationFactoryError.undefinedParaId
            }

            self?.setParaId(paraId, for: chainId)

            return paraId
        }

        updateOperation.addDependency(wrapper.targetOperation)

        let dependencies = [coderFactoryOperation] + wrapper.allOperations

        return CompoundOperationWrapper(targetOperation: updateOperation, dependencies: dependencies)
    }
}
