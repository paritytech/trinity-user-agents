import PolkadotUI
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

    /// The peer's image, loaded over its placeholder. A `refresh` rereads a public profile's avatar.
    func imageViewModel(
        using productFactory: ProductIconViewModelMaking,
        publicProfileFactory: PublicProfileAvatarViewModelMaking,
        refresh: Bool = false
    ) -> (any ImageViewModelProtocol)? {
        switch self {
        case let .product(domain):
            productFactory.createViewModel(for: domain)
        case let .publicProfile(username):
            publicProfileFactory.createViewModel(for: username, refresh: refresh)
        case .image,
             .bot:
            nil
        }
    }
}
