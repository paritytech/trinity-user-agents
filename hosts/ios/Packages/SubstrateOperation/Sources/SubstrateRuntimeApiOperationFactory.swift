import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStateCall
import ChainStore

public class SubstrateRuntimeApiOperationFactory {
    public let chainRegistry: ChainResourceProtocol
    public let operationQueue: OperationQueue
    public let stateCallFactory = StateCallRequestFactory()

    public init(chainRegistry: ChainResourceProtocol, operationQueue: OperationQueue) {
        self.chainRegistry = chainRegistry
        self.operationQueue = operationQueue
    }
}

public enum SubstrateRuntimeApiOperationFactoryError: Error {
    case unexpectedParamsCount
}

public extension SubstrateRuntimeApiOperationFactory {
    func createRuntimeCallWrapper<R: Decodable>(
        for chainId: ChainId,
        path: StateCallPath,
        blockHash: BlockHash? = nil,
        paramsClosure: StateCallWithApiParamsClosure?
    ) -> CompoundOperationWrapper<R> {
        do {
            let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)
            let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)

            return stateCallFactory.createWrapper(
                path: path,
                paramsClosure: paramsClosure,
                runtimeProvider: runtimeProvider,
                connection: connection,
                operationQueue: operationQueue,
                at: blockHash
            )
        } catch {
            return CompoundOperationWrapper.createWithError(error)
        }
    }
}
