import Kingfisher
import PolkadotUI

final class PublicProfileAvatarImageOptionsFactory: ImageProcessingOptionsProducing {
    private let base: ImageProcessingOptionsProducing

    init(base: ImageProcessingOptionsProducing = ImageProcessingOptionsFactory()) {
        self.base = base
    }

    func options(for settings: ImageViewModelSettings, animated: Bool) -> KingfisherOptionsInfo {
        base.options(for: settings, animated: animated).filter { option in
            switch option {
            case .cacheOriginalImage,
                 .diskCacheExpiration: false
            default: true
            }
        } + [.cacheMemoryOnly, .memoryCacheExpiration(.never)]
    }
}
