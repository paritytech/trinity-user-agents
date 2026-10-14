import Kingfisher
import PolkadotUI
import UIKit

final class ProductIconImageViewModel {
    private let provider: ImageDataProvider
    private let optionsFactory: ImageProcessingOptionsProducing

    init(
        provider: ImageDataProvider,
        optionsFactory: ImageProcessingOptionsProducing = ImageProcessingOptionsFactory()
    ) {
        self.provider = provider
        self.optionsFactory = optionsFactory
    }
}

extension ProductIconImageViewModel: ImageViewModelProtocol {
    @MainActor
    func loadImage(
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated: Bool,
        completion: ((Bool) -> Void)?
    ) {
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

    @MainActor
    func cancel(on imageView: UIImageView) {
        imageView.kf.cancelDownloadTask()
    }
}
