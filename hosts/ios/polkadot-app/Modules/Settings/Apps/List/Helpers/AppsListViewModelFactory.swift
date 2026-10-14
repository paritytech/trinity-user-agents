import Foundation
import PolkadotUI
import Products

protocol AppsListViewModelMaking {
    func createItems(from products: [ResolvedProduct]) -> [AppsListViewLayout.Item]
}

final class AppsListViewModelFactory {
    private let iconViewModelFactory: ProductIconViewModelMaking

    init(iconViewModelFactory: ProductIconViewModelMaking) {
        self.iconViewModelFactory = iconViewModelFactory
    }
}

extension AppsListViewModelFactory: AppsListViewModelMaking {
    func createItems(from products: [ResolvedProduct]) -> [AppsListViewLayout.Item] {
        products.map { product in
            AppsListViewLayout.Item(
                id: product.id,
                name: product.displayName,
                subtitle: product.domainSubtitle,
                avatar: product.placeholderAvatar,
                icon: iconViewModelFactory.createViewModel(for: product.id)
            )
        }
    }
}
