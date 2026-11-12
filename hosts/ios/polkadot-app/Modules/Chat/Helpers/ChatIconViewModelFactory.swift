import PolkadotUI
import TrUAPIHost

final class ChatIconViewModelFactory {
    private let productFactory: ProductIconViewModelMaking
    private let publicProfileFactory: PublicProfileAvatarViewModelMaking
    private let options: PublicProfileResolutionOptions

    init(
        productFactory: ProductIconViewModelMaking,
        publicProfileFactory: PublicProfileAvatarViewModelMaking,
        options: PublicProfileResolutionOptions
    ) {
        self.productFactory = productFactory
        self.publicProfileFactory = publicProfileFactory
        self.options = options
    }

    func createViewModel(from icon: Chat.PeerMetadata.Icon) -> AnyImageViewModel? {
        switch icon {
        case let .product(domain):
            AnyImageViewModel(productFactory.createViewModel(for: domain))
        case let .publicProfile(username):
            AnyImageViewModel(publicProfileFactory.createViewModel(for: username, options: options))
        case .image,
             .bot:
            nil
        }
    }
}
