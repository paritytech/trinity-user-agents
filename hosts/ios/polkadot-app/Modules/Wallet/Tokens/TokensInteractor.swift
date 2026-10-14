import Foundation
import Operation_iOS
import SubstrateSdk
import ChainRegistry

class TokensInteractor {
    weak var tokensPresenter: TokensOutputProtocol?

    let chainRegistry: ChainRegistryProtocol
    let supportedTokensService: SupportedTokensServiceProtocol

    private(set) var supportedAssetIds: [ChainAssetId] = []
    private(set) var supportedAssets: [ChainModel.Id: [AssetModel.Id: ChainAsset]] = [:]

    init(chainRegistry: ChainRegistryProtocol, supportedTokensService: SupportedTokensServiceProtocol) {
        self.chainRegistry = chainRegistry
        self.supportedTokensService = supportedTokensService
    }

    private func provideSupportedAssets() {
        let chainAssets = supportedAssetIds.compactMap { chainAssetId in
            let assets = supportedAssets[chainAssetId.chainId]

            return assets?[chainAssetId.assetId]
        }

        MainActor.assumeIsolated {
            tokensPresenter?.didReceive(chainAssets: chainAssets)
        }
    }

    private func subscribeChains() {
        chainRegistry.chainsSubscribe(
            self,
            runningInQueue: .main
        ) { [weak self] changes in
            self?.handle(changes: changes)
            self?.provideSupportedAssets()
        }
    }

    private func updateSupportedAssets(for chain: ChainModel) {
        supportedAssets[chain.chainId] = nil

        let supportedAssetIdSet = Set(supportedAssetIds)

        let supportedChainAssets = chain.assets.reduce(into: [AssetModel.Id: ChainAsset]()) { accum, asset in
            let chainAssetId = ChainAssetId(chainId: chain.chainId, assetId: asset.assetId)

            if supportedAssetIdSet.contains(chainAssetId) {
                accum[asset.assetId] = ChainAsset(chain: chain, asset: asset)
            }
        }

        supportedAssets[chain.chainId] = supportedChainAssets
    }

    func handle(changes: [DataProviderChange<ChainModel>]) {
        for change in changes {
            switch change {
            case let .insert(item),
                 let .update(item):
                updateSupportedAssets(for: item)
            case let .delete(deletedIdentifier):
                supportedAssets[deletedIdentifier] = nil
            }
        }
    }

    func setup() {
        supportedTokensService.fetchAvailableTokens(runningCompletionIn: .main) { [weak self] result in
            switch result {
            case let .success(supportedAssetIds):
                self?.supportedAssetIds = supportedAssetIds
                self?.subscribeChains()
            case let .failure(error):
                MainActor.assumeIsolated {
                    self?.tokensPresenter?.didReceive(error: .fetchFailed(error))
                }
            }
        }
    }
}

extension TokensInteractor: TokensInputProtocol {}
