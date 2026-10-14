import Foundation
import Foundation_iOS
import Operation_iOS
import SubstrateSdk
import SubstrateStorageSubscription
import HydrationSdk
import CommonService
import ChainStore
import AssetsManagement
import ChainRegistry

struct WalletRemoteSubscriptionUpdate {
    let balance: AssetBalance?
    let blockHash: Data?
}

typealias WalletRemoteSubscriptionClosure = (Result<WalletRemoteSubscriptionUpdate, Error>) -> Void

protocol WalletRemoteSubscriptionProtocol {
    func subscribeBalance(
        for accountId: AccountId,
        chainAsset: ChainAsset,
        callbackQueue: DispatchQueue,
        callbackClosure: @escaping WalletRemoteSubscriptionClosure
    )

    func unsubscribe()
}

final class WalletRemoteSubscription {
    let chainRegistry: ChainRegistryProtocol
    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    private var unsubscribeClosure: (() -> Void)?

    init(
        chainRegistry: ChainRegistryProtocol,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.chainRegistry = chainRegistry
        self.operationQueue = operationQueue
        self.logger = logger
    }

    deinit {
        doUnsubscribe()
    }

    func doUnsubscribe() {
        unsubscribeClosure?()
        unsubscribeClosure = nil
    }
}

private extension WalletRemoteSubscription {
    func subscribeNativeBalance(
        for accountId: AccountId,
        chainAsset: ChainAsset,
        callbackQueue: DispatchQueue,
        callbackClosure: @escaping WalletRemoteSubscriptionClosure
    ) {
        do {
            let request = MapSubscriptionRequest(
                storagePath: SystemPallet.accountPath,
                localKey: "",
                keyParamClosure: {
                    BytesCodable(wrappedValue: accountId)
                }
            )

            let connection = try chainRegistry.getConnectionOrError(for: chainAsset.chain.chainId)
            let runtimeProvider = try chainRegistry.getRuntimeProviderOrError(for: chainAsset.chain.chainId)

            let subscription = CallbackBatchStorageSubscription<AccountInfoState>(
                requests: [
                    BatchStorageSubscriptionRequest(
                        innerRequest: request,
                        mappingKey: AccountInfoState.Key.accountInfo.rawValue
                    )
                ],
                connection: connection,
                runtimeService: runtimeProvider,
                repository: nil,
                operationQueue: operationQueue,
                callbackQueue: callbackQueue
            ) { result in
                switch result {
                case let .success(valueWithBlock):
                    let accountInfo = valueWithBlock.accountInfo.valueWhenDefined(else: nil)

                    let assetBalance = accountInfo.map { accountInfo in
                        AssetBalance(
                            accountInfo: accountInfo,
                            chainAssetId: chainAsset.chainAssetId,
                            accountId: accountId
                        )
                    }

                    let callbackValue = WalletRemoteSubscriptionUpdate(
                        balance: assetBalance,
                        blockHash: valueWithBlock.blockHash
                    )

                    callbackClosure(.success(callbackValue))
                case let .failure(error):
                    callbackClosure(.failure(error))
                }
            }

            unsubscribeClosure = {
                subscription.unsubscribe()
            }

            subscription.subscribe()
        } catch {
            dispatchInQueueWhenPossible(callbackQueue) {
                callbackClosure(.failure(error))
            }
        }
    }

    func prepareAssetsBalanceRequests(
        accountId: AccountId,
        extras: StatemineAssetExtras
    ) -> [BatchStorageSubscriptionRequest] {
        let accountRequest = DoubleMapSubscriptionRequest(
            storagePath: AssetsPallet.accountPath(from: extras.palletName),
            localKey: "",
            keyParamClosure: {
                (extras.assetId, BytesCodable(wrappedValue: accountId))
            },
            param1Encoder: AssetsPalletSerializer.subscriptionKeyEncoder(for: extras.assetId),
            param2Encoder: nil
        )

        let detailsRequest = MapSubscriptionRequest(
            storagePath: AssetsPallet.detailsPath(from: extras.palletName),
            localKey: "",
            keyParamClosure: {
                extras.assetId
            },
            paramEncoder: AssetsPalletSerializer.subscriptionKeyEncoder(for: extras.assetId)
        )

        return [
            BatchStorageSubscriptionRequest(
                innerRequest: accountRequest,
                mappingKey: AssetsPalletBalanceStateChange.Key.account.rawValue
            ),
            BatchStorageSubscriptionRequest(
                innerRequest: detailsRequest,
                mappingKey: AssetsPalletBalanceStateChange.Key.details.rawValue
            )
        ]
    }

    func subscribeAssetsAccountBalance(
        for accountId: AccountId,
        chainAsset: ChainAsset,
        extras: StatemineAssetExtras,
        callbackQueue: DispatchQueue,
        callbackClosure: @escaping WalletRemoteSubscriptionClosure
    ) {
        do {
            let requests = prepareAssetsBalanceRequests(accountId: accountId, extras: extras)
            var state = AssetsPalletBalanceState(account: nil, details: nil)

            let connection = try chainRegistry.getConnectionOrError(for: chainAsset.chain.chainId)
            let runtimeProvider = try chainRegistry.getRuntimeProviderOrError(for: chainAsset.chain.chainId)

            let subscription = CallbackBatchStorageSubscription<AssetsPalletBalanceStateChange>(
                requests: requests,
                connection: connection,
                runtimeService: runtimeProvider,
                repository: nil,
                operationQueue: operationQueue,
                callbackQueue: callbackQueue
            ) { result in
                switch result {
                case let .success(change):
                    state = state.applying(change: change)

                    let assetBalance = AssetBalance(
                        assetsAccount: state.account,
                        assetsDetails: state.details,
                        chainAssetId: chainAsset.chainAssetId,
                        accountId: accountId
                    )

                    callbackClosure(.success(.init(balance: assetBalance, blockHash: change.blockHash)))
                case let .failure(error):
                    callbackClosure(.failure(error))
                }
            }

            unsubscribeClosure = {
                subscription.unsubscribe()
            }

            subscription.subscribe()
        } catch {
            dispatchInQueueWhenPossible(callbackQueue) {
                callbackClosure(.failure(error))
            }
        }
    }

    func subscribeOrmlAccountBalance(
        for accountId: AccountId,
        chainAsset: ChainAsset,
        currencyIdScale: String,
        callbackQueue: DispatchQueue,
        callbackClosure: @escaping WalletRemoteSubscriptionClosure
    ) {
        do {
            let currencyId = try Data(hexString: currencyIdScale)

            let request = DoubleMapSubscriptionRequest(
                storagePath: OrmlPallet.ormlTokenAccount,
                localKey: "",
                keyParamClosure: {
                    (BytesCodable(wrappedValue: accountId), BytesCodable(wrappedValue: currencyId))
                },
                param1Encoder: nil,
                param2Encoder: { $0.wrappedValue }
            )

            let connection = try chainRegistry.getConnectionOrError(for: chainAsset.chain.chainId)
            let runtimeProvider = try chainRegistry.getRuntimeProviderOrError(for: chainAsset.chain.chainId)

            let subscription = CallbackBatchStorageSubscription<OrmlAccountState>(
                requests: [
                    BatchStorageSubscriptionRequest(
                        innerRequest: request,
                        mappingKey: OrmlAccountState.Key.account.rawValue
                    )
                ],
                connection: connection,
                runtimeService: runtimeProvider,
                repository: nil,
                operationQueue: operationQueue,
                callbackQueue: callbackQueue
            ) { result in
                switch result {
                case let .success(valueWithBlock):
                    let account = valueWithBlock.account.valueWhenDefined(else: nil)
                    let assetBalance = account.map { account in
                        AssetBalance(
                            ormlAccount: account,
                            chainAssetId: chainAsset.chainAssetId,
                            accountId: accountId
                        )
                    }

                    let callbackValue = WalletRemoteSubscriptionUpdate(
                        balance: assetBalance,
                        blockHash: valueWithBlock.blockHash
                    )

                    callbackClosure(.success(callbackValue))
                case let .failure(error):
                    callbackClosure(.failure(error))
                }
            }

            unsubscribeClosure = {
                subscription.unsubscribe()
            }

            subscription.subscribe()
        } catch {
            dispatchInQueueWhenPossible(callbackQueue) {
                callbackClosure(.failure(error))
            }
        }
    }

    func subscribeOrmlHydrationEvmAccountBalance(
        for accountId: AccountId,
        chainAsset: ChainAsset,
        callbackQueue: DispatchQueue,
        callbackClosure: @escaping WalletRemoteSubscriptionClosure
    ) {
        let pollingState = ChainPollingStateStore(
            runtimeConnectionStore: ChainRegistryRuntimeConnectionStore(
                chainId: chainAsset.chain.chainId,
                chainRegistry: chainRegistry
            ),
            operationQueue: operationQueue,
            logger: logger
        )

        let service = OrmlHydrationEvmSubscriptionService(
            chainAssetId: chainAsset.chainAssetId,
            accountId: accountId,
            trigger: pollingState,
            chainRegistry: chainRegistry,
            operationQueue: operationQueue,
            workingQueue: .global(),
            logger: logger,
            callbackQueue: callbackQueue
        ) { balance, blockHash in
            callbackClosure(.success(.init(balance: balance, blockHash: blockHash)))
        }

        unsubscribeClosure = {
            pollingState.throttle()
            service.throttle()
        }

        service.setup()
    }
}

extension WalletRemoteSubscription: WalletRemoteSubscriptionProtocol {
    func subscribeBalance(
        for accountId: AccountId,
        chainAsset: ChainAsset,
        callbackQueue: DispatchQueue,
        callbackClosure: @escaping WalletRemoteSubscriptionClosure
    ) {
        do {
            return try CustomAssetMapper(
                type: chainAsset.asset.type,
                typeExtras: chainAsset.asset.typeExtras
            ).mapAssetWithExtras(
                .init(
                    nativeHandler: {
                        self.subscribeNativeBalance(
                            for: accountId,
                            chainAsset: chainAsset,
                            callbackQueue: callbackQueue,
                            callbackClosure: callbackClosure
                        )
                    },
                    statemineHandler: { extras in
                        self.subscribeAssetsAccountBalance(
                            for: accountId,
                            chainAsset: chainAsset,
                            extras: extras,
                            callbackQueue: callbackQueue,
                            callbackClosure: callbackClosure
                        )
                    },
                    ormlHandler: { extras in
                        self.subscribeOrmlAccountBalance(
                            for: accountId,
                            chainAsset: chainAsset,
                            currencyIdScale: extras.currencyIdScale,
                            callbackQueue: callbackQueue,
                            callbackClosure: callbackClosure
                        )
                    },
                    ormlHydrationEvmHandler: { _ in
                        self.subscribeOrmlHydrationEvmAccountBalance(
                            for: accountId,
                            chainAsset: chainAsset,
                            callbackQueue: callbackQueue,
                            callbackClosure: callbackClosure
                        )
                    }
                )
            )
        } catch {
            callbackQueue.async { callbackClosure(.failure(error)) }
        }
    }

    func unsubscribe() {
        doUnsubscribe()
    }
}

private struct AccountInfoState: BatchStorageSubscriptionResult {
    enum Key: String {
        case accountInfo
    }

    let accountInfo: UncertainStorage<SystemPallet.AccountInfo?>
    let blockHash: BlockHashData?

    init(
        values: [BatchStorageSubscriptionResultValue],
        blockHashJson: JSON,
        context: [CodingUserInfoKey: Any]?
    ) throws {
        accountInfo = try UncertainStorage(
            values: values,
            mappingKey: Key.accountInfo.rawValue,
            context: context
        )

        blockHash = try blockHashJson.map(to: BlockHashData?.self, with: context)
    }
}

private struct AssetsPalletBalanceStateChange: BatchStorageSubscriptionResult {
    enum Key: String {
        case account
        case details
    }

    let account: UncertainStorage<AssetsPallet.Account?>
    let details: UncertainStorage<AssetsPallet.Details?>
    let blockHash: BlockHashData?

    init(
        values: [BatchStorageSubscriptionResultValue],
        blockHashJson: JSON,
        context: [CodingUserInfoKey: Any]?
    ) throws {
        account = try UncertainStorage(
            values: values,
            mappingKey: Key.account.rawValue,
            context: context
        )

        details = try UncertainStorage(
            values: values,
            mappingKey: Key.details.rawValue,
            context: context
        )

        blockHash = try blockHashJson.map(to: BlockHashData?.self, with: context)
    }
}

private struct AssetsPalletBalanceState {
    let account: AssetsPallet.Account?
    let details: AssetsPallet.Details?

    func applying(change: AssetsPalletBalanceStateChange) -> Self {
        .init(
            account: change.account.valueWhenDefined(else: account),
            details: change.details.valueWhenDefined(else: details)
        )
    }
}

private struct OrmlAccountState: BatchStorageSubscriptionResult {
    enum Key: String {
        case account
    }

    let account: UncertainStorage<OrmlAccount?>
    let blockHash: BlockHashData?

    init(
        values: [BatchStorageSubscriptionResultValue],
        blockHashJson: JSON,
        context: [CodingUserInfoKey: Any]?
    ) throws {
        account = try UncertainStorage(
            values: values,
            mappingKey: Key.account.rawValue,
            context: context
        )

        blockHash = try blockHashJson.map(to: BlockHashData?.self, with: context)
    }
}
