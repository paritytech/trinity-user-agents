import PolkadotUI

/// Keeps the header's loader while its logical icon stays the same.
final class ChatHeaderIconViewModelFactory {
    private let productFactory: ProductIconViewModelMaking
    private let publicProfileFactory: PublicProfileAvatarViewModelMaking
    private var current: (icon: Chat.PeerMetadata.Icon, viewModel: AnyImageViewModel?)?

    init(productFactory: ProductIconViewModelMaking, publicProfileFactory: PublicProfileAvatarViewModelMaking) {
        self.productFactory = productFactory
        self.publicProfileFactory = publicProfileFactory
    }

    func viewModel(for icon: Chat.PeerMetadata.Icon) -> AnyImageViewModel? {
        if let current, current.icon == icon {
            return current.viewModel
        }

        let viewModel = icon.imageViewModel(
            using: productFactory,
            publicProfileFactory: publicProfileFactory,
            options: .init(forceFresh: true, skipStale: false)
        )
        current = (icon, viewModel)
        return viewModel
    }
}
