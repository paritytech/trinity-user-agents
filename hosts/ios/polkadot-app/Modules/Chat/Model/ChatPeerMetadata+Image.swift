import PolkadotUI
import UIKit.UIImage

extension Chat.PeerMetadata.Icon {
    var image: UIImage? {
        switch self {
        case let .image(data):
            data.flatMap { UIImage(data: $0) }
        case .bot:
            .iconBot
        case .product:
            nil
        }
    }

    /// The image a product peer loads by its domain, drawn over the letter avatar.
    func imageViewModel(using factory: ProductIconViewModelMaking) -> (any ImageViewModelProtocol)? {
        guard case let .product(domain) = self else { return nil }

        return factory.createViewModel(for: domain)
    }
}
