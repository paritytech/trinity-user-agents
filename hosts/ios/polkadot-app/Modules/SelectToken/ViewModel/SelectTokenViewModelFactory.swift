import Foundation
import PolkadotUI
import ChainRegistry

protocol SelectTokenViewModelFactoryProtocol {
    func createViewModel(from chainAsset: ChainAsset) -> SelectTokenCellViewModel
}

final class SelectTokenViewModelFactory {
    let tokenStyleProvider: ChainAssetStyleProviding

    init(
        tokenStyleProvider: ChainAssetStyleProviding = ChainAssetStyleProvider()
    ) {
        self.tokenStyleProvider = tokenStyleProvider
    }
}

extension SelectTokenViewModelFactory: SelectTokenViewModelFactoryProtocol {
    func createViewModel(from chainAsset: ChainAsset) -> SelectTokenCellViewModel {
        let style = tokenStyleProvider.provide(for: chainAsset)

        return .chainAsset(.init(
            chainAssetId: chainAsset.chainAssetId,
            name: chainAsset.chain.name,
            symbol: chainAsset.asset.symbol,
            icon: style.icon
        ))
    }
}

private extension ChainAssetStyle {
    var icon: AnyImageViewModel? {
        logo.map { AnyImageViewModel(StaticImageViewModel(image: $0)) }
    }
}
