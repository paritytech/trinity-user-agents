#if TESTNET_FEATURE
    import Foundation
    import SubstrateSdk
    import ExtrinsicService
    import NovaCrypto
    import ChainRegistry
    import BigInt
    import AssetsManagement
    import StructuredConcurrency

    final class TopUpService {
        enum ServiceError: Error {
            case unsupportedAsset
        }

        private let chainRegistry: ChainRegistryProtocol
        private let extrinsicMonitorFactory: ExtrinsicSubmissionMonitorFacadeProtocol
        private let assetStorageInfoFactory: AssetStorageInfoOperationFactoryProtocol
        private let operationQueue: OperationQueue
        private let chainAsset: ChainAsset

        private var assetStorageInfo: AssetStorageInfo?

        init(
            chainAsset: ChainAsset,
            chainRegistry: ChainRegistryProtocol,
            extrinsicMonitorFactory: ExtrinsicSubmissionMonitorFacadeProtocol,
            assetStorageInfoFactory: AssetStorageInfoOperationFactoryProtocol,
            operationQueue: OperationQueue
        ) {
            self.chainAsset = chainAsset
            self.chainRegistry = chainRegistry
            self.extrinsicMonitorFactory = extrinsicMonitorFactory
            self.assetStorageInfoFactory = assetStorageInfoFactory
            self.operationQueue = operationQueue
        }

        private func getAssetStorageInfo() async throws -> AssetStorageInfo {
            guard let assetStorageInfo else {
                let info = try await assetStorageInfoFactory.createStorageInfoWrapper(
                    from: chainAsset.asset
                )
                .asyncExecute()

                assetStorageInfo = info
                return info
            }

            return assetStorageInfo
        }

        private func builderClosure(
            destination: AccountId,
            storageInfo: AssetStorageInfo,
            amount: Balance
        ) -> ExtrinsicBuilderClosure {
            { builder in
                switch storageInfo {
                case let .statemine(info):
                    let call = AssetsPallet.Transfer(
                        assetId: info.assetId,
                        target: .accoundId(destination),
                        amount: amount
                    )

                    return try builder.adding(call: call.runtimeCall())
                case let .native(info):
                    let call = BalancesPallet.Transfer(
                        dest: .accoundId(destination),
                        value: amount
                    )

                    let runtimeCall = RuntimeCall(
                        moduleName: info.transferCallPath.moduleName,
                        callName: info.transferCallPath.callName,
                        args: call
                    )

                    return try builder.adding(call: runtimeCall)
                case .orml,
                     .ormlHydrationEvm:
                    throw ServiceError.unsupportedAsset
                }
            }
        }

        func topUp(_ wallet: MetaAccountModelProtocol, amount: Amount = .integer(50)) async throws {
            try await markStallRegion("Faucet topup") {
                let amount: Balance =
                    switch amount {
                    case let .integer(value):
                        Decimal(min(50, value)).toSubstrateAmount(precision: Int16(chainAsset.asset.precision))!
                    case let .plank(value):
                        min(
                            Decimal(50).toSubstrateAmount(precision: Int16(chainAsset.asset.precision))!,
                            value
                        )
                    }

                let destination = try wallet.fetchAccount(for: chainAsset.chain).accountId

                let extrinsicMonitor = try extrinsicMonitorFactory.createMonitorFactory(
                    chain: chainAsset.chain
                )

                let originFactory = SignedExtrinsicOriginFactory(
                    chainRegistry: chainRegistry,
                    operationQueue: operationQueue,
                    logger: Logger.shared
                )

                let origin = try originFactory.extrinsicOriginDefiner(
                    from: AppConfig.topupOrigin,
                    chain: chainAsset.chain
                )

                let assetStorageInfo = try await getAssetStorageInfo()

                let closure = builderClosure(destination: destination, storageInfo: assetStorageInfo, amount: amount)

                try await extrinsicMonitor.submitAndMonitorWrapper(
                    extrinsicBuilderClosure: closure,
                    origin: origin,
                    params: .empty
                )
                .asyncExecute()
                .ensureSuccess()
            }
        }
    }

    extension TopUpService {
        enum Amount {
            case plank(Balance)
            case integer(UInt)
        }
    }

    private extension ExtrinsicSubmissionParams {
        static var empty: Self {
            ExtrinsicSubmissionParams(feeAssetId: nil, eventsMatcher: nil)
        }
    }

    extension TopUpService {
        static func create(for asset: ChainAssetId) -> TopUpService? {
            let chainRegistry = ChainRegistryFacade.sharedRegistry

            guard
                let chain = chainRegistry.getChain(for: asset.chainId),
                let chainAsset = chain.chainAsset(for: asset.assetId),
                let runtimeProvider = chainRegistry.getRuntimeProvider(for: chainAsset.chain.chainId)
            else {
                return nil
            }

            let storageFacade = SubstrateDataStorageFacade.shared
            let queue = OperationManagerFacade.sharedDefaultQueue

            let extrinsicMonitorFactory = ExtrinsicSubmissionMonitorFacade(
                chainRegistry: chainRegistry,
                substrateStorageFacade: storageFacade,
                operationQueue: queue
            )

            let assetStorageInfoFactory = AssetStorageInfoOperationFactory(
                chainRegistry: chainRegistry,
                runtimeProvider: runtimeProvider,
                operationQueue: queue
            )

            return TopUpService(
                chainAsset: chainAsset,
                chainRegistry: chainRegistry,
                extrinsicMonitorFactory: extrinsicMonitorFactory,
                assetStorageInfoFactory: assetStorageInfoFactory,
                operationQueue: queue
            )
        }
    }
#endif
