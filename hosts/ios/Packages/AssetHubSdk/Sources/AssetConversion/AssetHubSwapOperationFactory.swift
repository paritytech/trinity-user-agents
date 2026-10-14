import Foundation
import Foundation_iOS
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import BigInt
import AssetExchange

public protocol AssetHubSwapOperationFactoryProtocol: AssetQuoteFactoryProtocol {
    func availableDirections() -> CompoundOperationWrapper<[ChainAssetId: Set<ChainAssetId>]>
    func availableDirectionsForAsset(_ chainAssetId: ChainAssetId) -> CompoundOperationWrapper<Set<ChainAssetId>>
    func canPayFee(in chainAssetId: ChainAssetId) -> CompoundOperationWrapper<Bool>
}

public final class AssetHubSwapOperationFactory {
    static let sellQuoteApi = "AssetConversionApi_quote_price_exact_tokens_for_tokens"
    static let buyQuoteApi = "AssetConversionApi_quote_price_exact_tokens_for_tokens"

    let chain: ChainProtocol
    let runtimeService: RuntimeCodingServiceProtocol
    let connection: JSONRPCEngine
    let tokenConverter: AssetHubTokenConverting
    let bulkMapperFactory: AssetHubBulkTokensMapperFactoryProtocol
    let operationQueue: OperationQueue

    public init(
        chain: ChainProtocol,
        runtimeService: RuntimeCodingServiceProtocol,
        connection: JSONRPCEngine,
        tokenConverter: AssetHubTokenConverting,
        bulkMapperFactory: AssetHubBulkTokensMapperFactoryProtocol,
        operationQueue: OperationQueue
    ) {
        self.chain = chain
        self.runtimeService = runtimeService
        self.connection = connection
        self.operationQueue = operationQueue
        self.tokenConverter = tokenConverter
        self.bulkMapperFactory = bulkMapperFactory
    }

    private func fetchAllPairsWrapper(
        dependingOn codingFactoryOperation: BaseOperation<RuntimeCoderFactoryProtocol>,
        chain: ChainProtocol,
        tokenConverter: AssetHubTokenConverting
    ) -> CompoundOperationWrapper<[AssetConversionPallet.PoolAssetPair]> {
        let prefixEncodingOperation = UnkeyedEncodingOperation(
            path: AssetConversionPallet.poolsPath,
            storageKeyFactory: StorageKeyFactory()
        )

        prefixEncodingOperation.configurationBlock = {
            do {
                prefixEncodingOperation.codingFactory = try codingFactoryOperation.extractNoCancellableResultData()
            } catch {
                prefixEncodingOperation.result = .failure(error)
            }
        }

        let keysFetchOperation = StorageKeysQueryService(
            connection: connection,
            operationManager: OperationManager(operationQueue: operationQueue),
            prefixKeyClosure: { try prefixEncodingOperation.extractNoCancellableResultData() },
            mapper: AnyMapper(mapper: IdentityMapper())
        ).longrunOperation()

        keysFetchOperation.addDependency(prefixEncodingOperation)

        let decodingOperation = StorageKeyDecodingOperation<AssetConversionPallet.AssetIdPair>(
            path: AssetConversionPallet.poolsPath
        )

        decodingOperation.configurationBlock = {
            do {
                decodingOperation.codingFactory = try codingFactoryOperation.extractNoCancellableResultData()
                decodingOperation.dataList = try keysFetchOperation.extractNoCancellableResultData()
            } catch {
                decodingOperation.result = .failure(error)
            }
        }

        decodingOperation.addDependency(keysFetchOperation)

        let mappingOperation = ClosureOperation<[AssetConversionPallet.PoolAssetPair]> {
            let decodedPairs = try decodingOperation.extractNoCancellableResultData()

            return decodedPairs.compactMap { assetIdPair in
                guard
                    let asset1 = tokenConverter.convertFromMultilocation(
                        assetIdPair.asset1,
                        chain: chain
                    ) else {
                    return nil
                }

                guard
                    let asset2 = tokenConverter.convertFromMultilocation(
                        assetIdPair.asset2,
                        chain: chain
                    ) else {
                    return nil
                }

                return .init(asset1: asset1, asset2: asset2)
            }
        }

        mappingOperation.addDependency(decodingOperation)

        return CompoundOperationWrapper(
            targetOperation: mappingOperation,
            dependencies: [prefixEncodingOperation, keysFetchOperation, decodingOperation]
        )
    }

    private func mapRemotePairsOperation(
        for chain: ChainProtocol,
        dependingOn codingFactoryOperation: BaseOperation<RuntimeCoderFactoryProtocol>,
        remotePairsOperation: BaseOperation<[AssetConversionPallet.PoolAssetPair]>,
        bulkMappingFactory: AssetHubBulkTokensMapperFactoryProtocol
    ) -> BaseOperation<[ChainAssetId: Set<ChainAssetId>]> {
        ClosureOperation<[ChainAssetId: Set<ChainAssetId>]> {
            let codingFactory = try codingFactoryOperation.extractNoCancellableResultData()
            let remotePairs = try remotePairsOperation.extractNoCancellableResultData()

            let bulkMapper = try bulkMappingFactory.createBulkMapper(for: chain, codingFactory: codingFactory)

            let initPairsStore = [ChainAssetId: Set<ChainAssetId>]()
            let result = remotePairs.reduce(into: initPairsStore) { store, remotePair in
                guard
                    let asset1 = bulkMapper.convertPoolAsset(remotePair.asset1),
                    let asset2 = bulkMapper.convertPoolAsset(remotePair.asset2) else {
                    return
                }

                store[asset1] = Set([asset2]).union(store[asset1] ?? [])
                store[asset2] = Set([asset1]).union(store[asset2] ?? [])
            }

            return result
        }
    }
}

extension AssetHubSwapOperationFactory: AssetHubSwapOperationFactoryProtocol {
    public func availableDirections() -> CompoundOperationWrapper<[ChainAssetId: Set<ChainAssetId>]> {
        let codingFactoryOperation = runtimeService.fetchCoderFactoryOperation()
        let fetchRemoteWrapper = fetchAllPairsWrapper(
            dependingOn: codingFactoryOperation,
            chain: chain,
            tokenConverter: tokenConverter
        )

        let mappingOperation = mapRemotePairsOperation(
            for: chain,
            dependingOn: codingFactoryOperation,
            remotePairsOperation: fetchRemoteWrapper.targetOperation,
            bulkMappingFactory: bulkMapperFactory
        )

        fetchRemoteWrapper.addDependency(operations: [codingFactoryOperation])
        mappingOperation.addDependency(fetchRemoteWrapper.targetOperation)

        let dependencies = [codingFactoryOperation] + fetchRemoteWrapper.allOperations

        return CompoundOperationWrapper(targetOperation: mappingOperation, dependencies: dependencies)
    }

    public func availableDirectionsForAsset(_ chainAssetId: ChainAssetId)
        -> CompoundOperationWrapper<Set<ChainAssetId>> {
        let allDirectionsWrapper = availableDirections()

        let mappingOperation = ClosureOperation<Set<ChainAssetId>> {
            let allChainAssets = try allDirectionsWrapper.targetOperation.extractNoCancellableResultData()

            return allChainAssets[chainAssetId] ?? []
        }

        mappingOperation.addDependency(allDirectionsWrapper.targetOperation)

        return CompoundOperationWrapper(
            targetOperation: mappingOperation,
            dependencies: allDirectionsWrapper.allOperations
        )
    }

    public func quote(for args: AssetConversion.QuoteArgs) -> CompoundOperationWrapper<AssetConversion.Quote> {
        let codingFactoryOperation = runtimeService.fetchCoderFactoryOperation()

        let request = AssetHubSwapRequestBuilder(
            chain: chain,
            tokenConverter: tokenConverter
        ).build(args: args) {
            try codingFactoryOperation.extractNoCancellableResultData()
        }

        let quoteOperation = JSONRPCOperation<StateCallRpc.Request, String>(
            engine: connection,
            method: StateCallRpc.method
        )

        quoteOperation.parameters = request

        quoteOperation.addDependency(codingFactoryOperation)

        let mappingOperation = ClosureOperation<AssetConversion.Quote> {
            let responseString = try quoteOperation.extractNoCancellableResultData()
            let codingFactory = try codingFactoryOperation.extractNoCancellableResultData()

            let amount = try AssetHubSwapRequestSerializer.deserialize(
                quoteResponse: responseString,
                codingFactory: codingFactory
            )

            return .init(args: args, amount: amount, context: nil)
        }

        mappingOperation.addDependency(quoteOperation)

        let dependencies = [codingFactoryOperation, quoteOperation]

        return CompoundOperationWrapper(targetOperation: mappingOperation, dependencies: dependencies)
    }

    public func canPayFee(in chainAssetId: ChainAssetId) -> CompoundOperationWrapper<Bool> {
        guard let utilityAssetId = chain.utilityChainAssetId() else {
            return CompoundOperationWrapper.createWithResult(false)
        }

        if chainAssetId == utilityAssetId {
            return CompoundOperationWrapper.createWithResult(true)
        }

        let availableDirectionsWrapper = availableDirectionsForAsset(chainAssetId)

        let mergeOperation = ClosureOperation<Bool> {
            let directions = try availableDirectionsWrapper.targetOperation.extractNoCancellableResultData()

            return directions.contains(utilityAssetId)
        }

        mergeOperation.addDependency(availableDirectionsWrapper.targetOperation)

        return availableDirectionsWrapper.insertingTail(operation: mergeOperation)
    }
}
