import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStateCall
import StructuredConcurrency
import ChainStore

public protocol HopRuntimeApiProtocol {
    /// Calls `HopRuntimeApi_can_account_promote(who, data_len)` on [chainId]. The HOP node applies the
    /// same check to `hop_submit`. Returns `true` when the account holds an unexpired `TransactionStorage`
    /// authorization; the runtime ignores the spent extent and `data_len` for the result, so `data_len`
    /// is encoded as `0`.
    func canAccountPromote(chainId: ChainId, account: Data) async throws -> Bool
}

public final class HopRuntimeApi: SubstrateRuntimeApiOperationFactory {
    static let apiName = "HopRuntimeApi"
    static let methodName = "can_account_promote"
}

extension HopRuntimeApi: HopRuntimeApiProtocol {
    public func canAccountPromote(
        chainId: ChainId,
        account: Data
    ) async throws -> Bool {
        let wrapper: CompoundOperationWrapper<Bool> = createRuntimeCallWrapper(
            for: chainId,
            path: StateCallPath(module: Self.apiName, method: Self.methodName)
        ) { runtimeApi, encoder, context in
            guard runtimeApi.method.inputs.count == 2 else {
                throw SubstrateRuntimeApiOperationFactoryError.unexpectedParamsCount
            }

            let whoType = runtimeApi.method.inputs[0].paramType
            try encoder.append(
                BytesCodable(wrappedValue: account),
                ofType: whoType.asTypeId(),
                with: context.toRawContext()
            )

            // The runtime ignores data_len for the result; encode 0 to satisfy the call signature.
            let dataLenType = runtimeApi.method.inputs[1].paramType
            try encoder.append(
                StringScaleMapper(value: UInt32(0)),
                ofType: dataLenType.asTypeId(),
                with: context.toRawContext()
            )
        }

        return try await wrapper.asyncExecute()
    }
}
