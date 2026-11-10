import PolkadotUI
import TrUAPIHost
import UIKit.UIImage

extension Chat.PeerMetadata.Icon {
    var image: UIImage? {
        switch self {
        case let .image(data):
            data.flatMap { UIImage(data: $0) }
        case .bot:
            .iconBot
        case .product,
             .publicProfile:
            nil
        }
    }

    /// The peer's image, loaded over its placeholder.
    func imageViewModel(
        using productFactory: ProductIconViewModelMaking,
        publicProfileFactory: PublicProfileAvatarViewModelMaking,
        options: PublicProfileResolutionOptions
    ) -> AnyImageViewModel? {
        switch self {
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
