import PolkadotUI

/// Keeps the header's icon while the peer's icon stays the same. The header loads its icon again on every chat
/// update, so a public profile avatar is reread once, when the chat opens.
final class ChatHeaderIconViewModelFactory {
    private let productFactory: ProductIconViewModelMaking
    private let publicProfileFactory: PublicProfileAvatarViewModelMaking
    private var current: (icon: Chat.PeerMetadata.Icon, viewModel: (any ImageViewModelProtocol)?)?

    init(productFactory: ProductIconViewModelMaking, publicProfileFactory: PublicProfileAvatarViewModelMaking) {
        self.productFactory = productFactory
        self.publicProfileFactory = publicProfileFactory
    }

    func viewModel(for icon: Chat.PeerMetadata.Icon) -> (any ImageViewModelProtocol)? {
        if let current, current.icon == icon {
            return current.viewModel
        }

        let viewModel = icon.imageViewModel(
            using: productFactory,
            publicProfileFactory: publicProfileFactory,
            refresh: true
        )
        current = (icon, viewModel)
        return viewModel
    }
}
