import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStateCall
import StructuredConcurrency
import ChainStore

public protocol TaggedTransactionQueueApiProtocol {
    func validateTransaction(
        chainId: ChainId,
        source: TransactionSource,
        extrinsic: Data,
        at blockHash: BlockHashData
    ) async throws -> TransactionValidity
}

public final class TaggedTransactionQueueApi: SubstrateRuntimeApiOperationFactory {
    static let apiName = "TaggedTransactionQueue"
    static let methodName = "validate_transaction"
}

extension TaggedTransactionQueueApi: TaggedTransactionQueueApiProtocol {
    public func validateTransaction(
        chainId: ChainId,
        source: TransactionSource,
        extrinsic: Data,
        at blockHash: BlockHashData
    ) async throws -> TransactionValidity {
        let wrapper: CompoundOperationWrapper<Substrate.Result<JSON, TransactionValidityError>>
        wrapper = createRuntimeCallWrapper(
            for: chainId,
            path: StateCallPath(module: Self.apiName, method: Self.methodName),
            blockHash: blockHash.toHex(includePrefix: true)
        ) { runtimeApi, encoder, context in
            guard runtimeApi.method.inputs.count == 3 else {
                throw SubstrateRuntimeApiOperationFactoryError.unexpectedParamsCount
            }

            let sourceType = runtimeApi.method.inputs[0].paramType
            try encoder.append(source, ofType: sourceType.asTypeId(), with: context.toRawContext())

            let txType = runtimeApi.method.inputs[1].paramType
            try encoder.append(
                BytesCodable(wrappedValue: extrinsic),
                ofType: txType.asTypeId(),
                with: context.toRawContext()
            )

            let blockHashType = runtimeApi.method.inputs[2].paramType
            try encoder.append(
                BytesCodable(wrappedValue: blockHash),
                ofType: blockHashType.asTypeId(),
                with: context.toRawContext()
            )
        }

        let result = try await wrapper.asyncExecute()

        return switch result {
        case .success:
            .valid
        case let .failure(error):
            switch error {
            case let .invalid(reason): .invalid(reason)
            case let .unknown(reason): .unknown(reason)
            }
        }
    }
}
