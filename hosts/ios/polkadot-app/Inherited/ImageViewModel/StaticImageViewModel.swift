import Kingfisher
import UIKit
import PolkadotUI

final class StaticImageViewModel: ImageViewModelProtocol {
    let image: UIImage?
    private let optionsFactory: ImageProcessingOptionsProducing

    init(image: UIImage?, optionsFactory: ImageProcessingOptionsProducing = ImageProcessingOptionsFactory()) {
        self.image = image
        self.optionsFactory = optionsFactory
    }

    @MainActor func loadImage(
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated: Bool,
        completion: ((Bool) -> Void)?
    ) {
        guard let image, let imageData = image.pngData() else {
            imageView.image = nil
            completion?(false)
            return
        }

        let provider = RawImageDataProvider(data: imageData, cacheKey: "local_\(UUID().uuidString)")
        let options = optionsFactory.options(for: settings, animated: animated)
        imageView.kf.setImage(with: provider, options: options) { result in
            switch result {
            case .success:
                completion?(true)
            case .failure:
                completion?(false)
            }
        }
    }

    @MainActor func cancel(on imageView: UIImageView) {
        imageView.kf.cancelDownloadTask()
        imageView.image = nil
    }
}
