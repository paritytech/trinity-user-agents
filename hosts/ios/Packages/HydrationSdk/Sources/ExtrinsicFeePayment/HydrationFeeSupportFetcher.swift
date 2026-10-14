import Foundation
import Operation_iOS
import SubstrateSdk
import AssetExchange
import SubstrateStorageQuery
import SDKLogger

public final class HydraExchangeFeeSupportFetcher {
    let chain: ChainProtocol
    let operationQueue: OperationQueue
    let connection: JSONRPCEngine
    let runtimeProvider: RuntimeCodingServiceProtocol
    let tokenConverter: HydrationTokenConverting
    let logger: SDKLoggerProtocol

    public init(
        chain: ChainProtocol,
        tokenConverter: HydrationTokenConverting,
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeCodingServiceProtocol,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol
    ) {
        self.chain = chain
        self.tokenConverter = tokenConverter
        self.operationQueue = operationQueue
        self.connection = connection
        self.runtimeProvider = runtimeProvider
        self.logger = logger
    }
}

extension HydraExchangeFeeSupportFetcher: AssetExchangeFeeSupportFetching {
    public var identifier: String { "hydra-fee-\(chain.chainId)" }

    public func createFeeSupportWrapper() -> CompoundOperationWrapper<AssetExchangeFeeSupporting> {
        let codingFactoryOperation = runtimeProvider.fetchCoderFactoryOperation()

        let keysFactory = StorageKeysOperationFactory(operationQueue: operationQueue)
        let assetsFetchWrapper: CompoundOperationWrapper<[HydraDx.AssetsKey]> = keysFactory.createKeysFetchWrapper(
            by: HydraDx.feeCurrenciesPath,
            codingFactoryClosure: { try codingFactoryOperation.extractNoCancellableResultData() },
            connection: connection
        )

        assetsFetchWrapper.addDependency(operations: [codingFactoryOperation])

        let mapOperation = ClosureOperation<AssetExchangeFeeSupporting> {
            let allAssets = try assetsFetchWrapper.targetOperation.extractNoCancellableResultData()
            let codingFactory = try codingFactoryOperation.extractNoCancellableResultData()

            let remoteLocalMapping = try self.tokenConverter.convertToRemoteLocalMapping(
                remoteAssets: Set(allAssets.map(\.assetId)),
                chain: self.chain,
                codingFactory: codingFactory
            )

            let localFeeAssetIds = Set(remoteLocalMapping.values)

            return AssetExchangeFeeSupport(supportedAssets: localFeeAssetIds)
        }

        mapOperation.addDependency(codingFactoryOperation)
        mapOperation.addDependency(assetsFetchWrapper.targetOperation)

        return assetsFetchWrapper
            .insertingHead(operations: [codingFactoryOperation])
            .insertingTail(operation: mapOperation)
    }
}
