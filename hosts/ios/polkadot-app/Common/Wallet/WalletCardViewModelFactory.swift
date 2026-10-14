import Foundation
import PolkadotUI
import ChainRegistry

protocol WalletCardViewModelFactoryProtocol {
    func createAssetViewModel(from asset: ChainAsset) -> WalletCardCreateViewModel
}

final class WalletCardViewModelFactory {
    let tokenStyleProvider: ChainAssetStyleProviding

    init(tokenStyleProvider: ChainAssetStyleProviding = ChainAssetStyleProvider()) {
        self.tokenStyleProvider = tokenStyleProvider
    }
}

extension WalletCardViewModelFactory: WalletCardViewModelFactoryProtocol {
    func createAssetViewModel(from asset: ChainAsset) -> WalletCardCreateViewModel {
        let style = tokenStyleProvider.provide(for: asset)

        return WalletCardCreateViewModel(
            info: .init(name: style.displayTitle),
            style: .init(background: .init(backgroundColor: style.brandColor))
        )
    }
}
