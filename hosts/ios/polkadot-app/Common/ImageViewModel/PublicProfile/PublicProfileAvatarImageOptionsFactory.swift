import Kingfisher
import PolkadotUI

final class PublicProfileAvatarImageOptionsFactory: ImageProcessingOptionsProducing {
    private let base: ImageProcessingOptionsProducing

    init(base: ImageProcessingOptionsProducing = ImageProcessingOptionsFactory()) {
        self.base = base
    }

    func options(for settings: ImageViewModelSettings, animated: Bool) -> KingfisherOptionsInfo {
        base.options(for: settings, animated: animated).filter { option in
            if case .cacheOriginalImage = option {
                return false
            }
            return true
        } + [
            .cacheMemoryOnly,
            .memoryCacheExpiration(.seconds(3_600)),
            .memoryCacheAccessExtendingExpiration(.none)
        ]
    }
}
