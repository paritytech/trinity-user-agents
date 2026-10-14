import Foundation
import Operation_iOS
import SubstrateSdk
import AssetExchange

final class CrosschainExchangeOperationPrototype: AssetExchangeBaseOperationPrototype {
    let host: CrosschainExchangeHostProtocol

    init(assetIn: ChainAssetProtocol, assetOut: ChainAssetProtocol, host: CrosschainExchangeHostProtocol) {
        self.host = host

        super.init(assetIn: assetIn, assetOut: assetOut)
    }

    private func createXcmPartiesResolutionWrapper(
        for destinationAccount: AccountProtocol
    ) -> CompoundOperationWrapper<XcmTransferParties> {
        host.resolutionFactory.createResolutionWrapper(
            for: assetIn.chainAssetId,
            transferDestinationId: .init(
                chainAssetId: assetOut.chainAssetId,
                accountId: destinationAccount.accountId
            ),
            xcmTransfers: host.xcmTransfers
        )
    }
}

private extension CrosschainExchangeOperationPrototype {
    private func isChainWithExpensiveCrossChain(chainId: ChainId) -> Bool {
        host.chainsWithExpensiveCrosschain.contains(chainId)
    }
}

extension CrosschainExchangeOperationPrototype: AssetExchangeOperationPrototypeProtocol {
    func estimatedCostInUsdt(using _: AssetExchageUsdtConverting) throws -> Decimal {
        var cost: Decimal = 0

        let chainInExpensive = isChainWithExpensiveCrossChain(
            chainId: assetIn.chainInterface.chainId
        )
        let chainOutExpensive = isChainWithExpensiveCrossChain(
            chainId: assetOut.chainInterface.chainId
        )

        if chainInExpensive {
            cost += 0.15
        }

        if chainOutExpensive {
            cost += 0.1
        }

        if !chainInExpensive || !chainOutExpensive {
            cost += 0.01
        }

        return cost
    }

    func estimatedExecutionTimeWrapper() -> CompoundOperationWrapper<TimeInterval> {
        do {
            let destinationAccount = try host.wallet.fetchAccount(
                for: assetOut.chainInterface
            )

            let resolutionWrapper = createXcmPartiesResolutionWrapper(for: destinationAccount)

            let estimationTimeWrapper = OperationCombiningService<TimeInterval>.compoundNonOptionalWrapper(
                operationQueue: host.operationQueue
            ) {
                let partiesResolution = try resolutionWrapper.targetOperation.extractNoCancellableResultData()

                let originChain = partiesResolution.originChain
                let destinationChain = partiesResolution.destinationChain
                let reserveChain = partiesResolution.reserveChain

                let relaychainId = [originChain, destinationChain, reserveChain]
                    .compactMap(\.parentId)
                    .first ?? originChain.chainId

                var participatingChains: [ChainId] = [originChain.chainId]

                if originChain.chainId != reserveChain.chainId {
                    participatingChains.append(reserveChain.chainId)

                    if !originChain.isRelaychain, !reserveChain.isRelaychain {
                        participatingChains.append(relaychainId)
                    }
                }

                if reserveChain.chainId != destinationChain.chainId {
                    participatingChains.append(destinationChain.chainId)

                    if !reserveChain.isRelaychain, !destinationChain.isRelaychain {
                        participatingChains.append(relaychainId)
                    }
                }

                guard !participatingChains.isEmpty else {
                    return .createWithResult(0)
                }

                return self.host.executionTimeEstimator.totalTimeWrapper(for: participatingChains)
            }

            estimationTimeWrapper.addDependency(wrapper: resolutionWrapper)

            return estimationTimeWrapper.insertingHead(operations: resolutionWrapper.allOperations)
        } catch {
            return .createWithError(error)
        }
    }
}
