import Foundation
import SubstrateSdk
import SubstrateStateCall
import Operation_iOS
import XcmDefinition

public protocol XcmPaymentOperationFactoryProtocol {
    func queryMessageWeight(
        for message: XcmUni.VersionedMessage,
        chainId: ChainId
    ) -> CompoundOperationWrapper<XcmPayment.WeightResult>

    func hasSupportWrapper(for chainId: ChainId) -> CompoundOperationWrapper<Bool>
}

public class XcmPaymentOperationFactory: SubstrateRuntimeApiOperationFactory {}

private extension XcmPaymentOperationFactory {
    func getWeightQueryPath() -> StateCallPath {
        StateCallPath(module: XcmPayment.apiName, method: "query_xcm_weight")
    }
}

extension XcmPaymentOperationFactory: XcmPaymentOperationFactoryProtocol {
    public func queryMessageWeight(
        for message: XcmUni.VersionedMessage,
        chainId: ChainId
    ) -> CompoundOperationWrapper<XcmPayment.WeightResult> {
        createRuntimeCallWrapper(
            for: chainId,
            path: getWeightQueryPath()
        ) { runtimeApi, encoder, context in
            let paramsCount = runtimeApi.method.inputs.count
            guard paramsCount == 1 else {
                throw SubstrateRuntimeApiOperationFactoryError.unexpectedParamsCount
            }

            let originType = runtimeApi.method.inputs[0].paramType

            try encoder.append(
                message,
                ofType: originType.asTypeId(),
                with: context.toRawContext()
            )
        }
    }

    public func hasSupportWrapper(for chainId: ChainId) -> CompoundOperationWrapper<Bool> {
        do {
            let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)
            let coderFactoryOperation = runtimeProvider.fetchCoderFactoryOperation()

            let methodPath = getWeightQueryPath()
            let mapOperation = ClosureOperation<Bool> {
                let coderFactory = try coderFactoryOperation.extractNoCancellableResultData()

                let method = coderFactory.metadata.getRuntimeApiMethod(
                    for: methodPath.module,
                    methodName: methodPath.method
                )

                return method != nil
            }

            mapOperation.addDependency(coderFactoryOperation)

            return CompoundOperationWrapper(
                targetOperation: mapOperation,
                dependencies: [coderFactoryOperation]
            )
        } catch {
            return .createWithError(error)
        }
    }
}
