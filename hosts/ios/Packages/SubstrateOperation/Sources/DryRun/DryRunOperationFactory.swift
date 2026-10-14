import Foundation
import SubstrateSdk
import SubstrateStateCall
import Operation_iOS
import XcmDefinition

public protocol DryRunOperationFactoryProtocol {
    func createDryRunCallWrapper(
        _ call: RuntimeCall<some Any>,
        origin: RuntimeCallOrigin,
        xcmVersion: Xcm.Version,
        chainId: ChainId
    ) -> CompoundOperationWrapper<DryRun.CallResult>

    func createDryRunXcmWrapper(
        from origin: XcmUni.VersionedLocation,
        xcm: XcmUni.VersionedMessage,
        chainId: ChainId
    ) -> CompoundOperationWrapper<DryRun.XcmResult>
}

public final class DryRunOperationFactory: SubstrateRuntimeApiOperationFactory {}

extension DryRunOperationFactory: DryRunOperationFactoryProtocol {
    public func createDryRunCallWrapper(
        _ call: RuntimeCall<some Any>,
        origin: RuntimeCallOrigin,
        xcmVersion: Xcm.Version,
        chainId: ChainId
    ) -> CompoundOperationWrapper<DryRun.CallResult> {
        createRuntimeCallWrapper(
            for: chainId,
            path: StateCallPath(module: DryRun.apiName, method: "dry_run_call")
        ) { runtimeApi, encoder, context in
            // dry run v2 has additional xcm version param
            let paramsCount = runtimeApi.method.inputs.count
            guard paramsCount == 2 || paramsCount == 3 else {
                throw SubstrateRuntimeApiOperationFactoryError.unexpectedParamsCount
            }

            let originType = runtimeApi.method.inputs[0].paramType

            try encoder.append(
                origin,
                ofType: originType.asTypeId(),
                with: context.toRawContext()
            )

            let callType = runtimeApi.method.inputs[1].paramType

            try encoder.append(
                call,
                ofType: callType.asTypeId(),
                with: context.toRawContext()
            )

            if paramsCount == 3 {
                let xcmVersionType = runtimeApi.method.inputs[2].paramType

                try encoder.append(
                    StringScaleMapper(value: xcmVersion.rawValue),
                    ofType: xcmVersionType.asTypeId(),
                    with: context.toRawContext()
                )
            }
        }
    }

    public func createDryRunXcmWrapper(
        from origin: XcmUni.VersionedLocation,
        xcm: XcmUni.VersionedMessage,
        chainId: ChainId
    ) -> CompoundOperationWrapper<DryRun.XcmResult> {
        createRuntimeCallWrapper(
            for: chainId,
            path: StateCallPath(module: DryRun.apiName, method: "dry_run_xcm")
        ) { runtimeApi, encoder, context in
            guard runtimeApi.method.inputs.count == 2 else {
                throw SubstrateRuntimeApiOperationFactoryError.unexpectedParamsCount
            }

            let originType = runtimeApi.method.inputs[0].paramType

            try encoder.append(
                origin,
                ofType: originType.asTypeId(),
                with: context.toRawContext()
            )

            let xcmType = runtimeApi.method.inputs[1].paramType

            try encoder.append(
                xcm,
                ofType: xcmType.asTypeId(),
                with: context.toRawContext()
            )
        }
    }
}
