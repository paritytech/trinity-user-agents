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
             .resource,
             .roomImage:
            nil
        }
    }

    /// The image a product peer or a product room loads on demand, drawn over the letter avatar.
    func imageViewModel(using factory: ProductIconViewModelMaking) -> (any ImageViewModelProtocol)? {
        switch self {
        case let .product(domain):
            factory.createViewModel(for: domain)
        case let .resource(source):
            factory.createViewModel(forResource: source)
        case let .roomImage(image):
            factory.createViewModel(forChatResource: image.resource)
        case .image,
             .bot:
            nil
        }
    }
}
