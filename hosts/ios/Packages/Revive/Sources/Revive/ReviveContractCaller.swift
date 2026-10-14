import BigInt
import ChainStore
import Foundation
import Operation_iOS
import StructuredConcurrency
import SubstrateOperation
import SubstrateSdk
import SubstrateStateCall
@preconcurrency import SubstrateStorageQuery

/// ``ReviveContractCalling`` over the runtime-API operation factory and a storage request factory, with
/// the chain's connection and runtime looked up by id on every call.
final class ReviveContractCaller: ReviveContractCalling, @unchecked Sendable {
    /// The largest weight the runtime accepts for a simulation, so a read is never cut short by the
    /// limit; the same number bounds the storage deposit.
    static let maxWeight: BigUInt = 184_467_440_737_090
    static let callPath = StateCallPath(module: "ReviveApi", method: "call")
    static let originalAccountPath = StorageCodingPath(moduleName: RevivePallet.name, itemName: "OriginalAccount")

    private let chainResource: ChainResourceProtocol
    private let runtimeApiFactory: SubstrateRuntimeApiOperationFactory
    private let storageRequestFactory: StorageRequestFactoryProtocol

    init(
        chainResource: ChainResourceProtocol,
        operationQueue: OperationQueue,
        storageRequestFactory: StorageRequestFactoryProtocol? = nil
    ) {
        self.chainResource = chainResource
        runtimeApiFactory = SubstrateRuntimeApiOperationFactory(
            chainRegistry: chainResource,
            operationQueue: operationQueue
        )
        self.storageRequestFactory = storageRequestFactory ?? StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )
    }

    func call(
        chainId: ChainId,
        origin: AccountId,
        contract: EvmAddress,
        input: Data,
        at blockHash: BlockHash?
    ) async throws -> ReviveDryRunResult {
        let arguments = CallArguments(origin: origin, contract: contract, input: input)
        let wrapper: CompoundOperationWrapper<ReviveDryRunResult> = runtimeApiFactory.createRuntimeCallWrapper(
            for: chainId,
            path: Self.callPath,
            blockHash: blockHash,
            paramsClosure: { runtimeApi, encoder, context in
                try Self.encode(arguments, for: runtimeApi, into: encoder, context: context)
            }
        )

        return try await wrapper.asyncExecute()
    }

    func isAccountMapped(chainId: ChainId, account: AccountId) async throws -> Bool {
        let connection = try chainResource.getRpcConnectionOrError(for: chainId)
        let runtimeProvider = try chainResource.getRuntimeCodingServiceOrError(for: chainId)
        let evmAddress = try account.toH160()

        let codingFactoryOperation = runtimeProvider.fetchCoderFactoryOperation()
        let wrapper: CompoundOperationWrapper<[StorageResponse<BytesCodable>]> = storageRequestFactory.queryItems(
            engine: connection,
            keyParams: { [BytesCodable(wrappedValue: evmAddress)] },
            factory: { try codingFactoryOperation.extractNoCancellableResultData() },
            storagePath: Self.originalAccountPath,
            options: StorageQueryListOptions()
        )
        wrapper.addDependency(operations: [codingFactoryOperation])

        let responses = try await wrapper.insertingHead(operations: [codingFactoryOperation]).asyncExecute()
        return responses.first?.value != nil
    }
}

extension ReviveContractCaller {
    struct CallArguments {
        let origin: AccountId
        let contract: EvmAddress
        let input: Data
    }

    /// The `ReviveApi_call` parameters, each under the type the runtime declares for its *name*, so a
    /// reordered or renamed signature is refused rather than mis-encoded.
    static func encode(
        _ arguments: CallArguments,
        for runtimeApi: RuntimeApiQueryResult,
        into encoder: DynamicScaleEncoding,
        context: RuntimeJsonContext
    ) throws {
        let types = try ParameterTypes(inputs: runtimeApi.method.inputs)
        let rawContext = context.toRawContext()
        let weight = Substrate.WeightV2(refTime: maxWeight, proofSize: maxWeight)

        try encoder.append(BytesCodable(wrappedValue: arguments.origin), ofType: types.origin, with: rawContext)
        try encoder.append(BytesCodable(wrappedValue: arguments.contract), ofType: types.dest, with: rawContext)
        try encoder.append(StringCodable(wrappedValue: BigUInt.zero), ofType: types.value, with: rawContext)
        try encoder.append(weight, ofType: types.weightLimit, with: rawContext)
        try encoder.append(StringCodable(wrappedValue: maxWeight), ofType: types.storageDepositLimit, with: rawContext)
        try encoder.append(BytesCodable(wrappedValue: arguments.input), ofType: types.inputData, with: rawContext)
    }
}

private extension ReviveContractCaller {
    struct ParameterTypes {
        let origin: String
        let dest: String
        let value: String
        let weightLimit: String
        let storageDepositLimit: String
        let inputData: String

        init(inputs: [RuntimeApiMethodParamMetadata]) throws {
            let typesByName = Dictionary(inputs.map { ($0.name, $0.paramType.asTypeId()) }) { first, _ in first }

            func type(named name: String) throws -> String {
                guard let type = typesByName[name] else {
                    throw ReviveContractError.unexpectedRuntimeApiSignature(missingArgument: name)
                }
                return type
            }

            origin = try type(named: "origin")
            dest = try type(named: "dest")
            value = try type(named: "value")
            weightLimit = try typesByName[RevivePallet.WeightLimitArgument.weightLimit.rawValue]
                ?? type(named: RevivePallet.WeightLimitArgument.gasLimit.rawValue)
            storageDepositLimit = try type(named: "storage_deposit_limit")
            inputData = try type(named: "input_data")
        }
    }
}
