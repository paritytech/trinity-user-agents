import UIKit.UIImage

extension Chat.PeerMetadata.Icon {
    var image: UIImage? {
        switch self {
        case let .image(data):
            data.flatMap { UIImage(data: $0) }
        case .bot:
            .iconBot
        }
    }
}
