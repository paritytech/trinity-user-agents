import UIKit
import PolkadotUI
import Kingfisher

final class LocalImageViewModel {
    private let imageProvider: ImageDataProvider
    private let optionsFactory: ImageProcessingOptionsProducing
    private let keepsCurrentImageWhileLoading: Bool

    /// When `keepsCurrentImageWhileLoading` is `true`, the caller must clear the image view's image
    /// whenever its logical content changes to prevent a previous image from appearing while loading.
    init(
        provider: ImageDataProvider,
        optionsFactory: ImageProcessingOptionsProducing = ImageProcessingOptionsFactory(),
        keepsCurrentImageWhileLoading: Bool
    ) {
        imageProvider = provider
        self.optionsFactory = optionsFactory
        self.keepsCurrentImageWhileLoading = keepsCurrentImageWhileLoading
    }
}

extension LocalImageViewModel: ChatMessageMediaPreviewProviding {
    var identifier: String {
        imageProvider.cacheKey
    }

    @MainActor
    func providePreview(for imageView: UIImageView, size: CGSize?) {
        let settings = ImageViewModelSettings(targetSize: size)
        loadImage(on: imageView, settings: settings, animated: true, completion: nil)
    }
}

extension LocalImageViewModel: ImageViewModelProtocol {
    @MainActor
    func loadImage(
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated: Bool,
        completion: ((Bool) -> Void)?
    ) {
        var options = optionsFactory.options(for: settings, animated: animated)
        if keepsCurrentImageWhileLoading {
            options.append(.keepCurrentImageWhileLoading)
        }
        imageView.kf.setImage(with: imageProvider, options: options) { result in
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
        imageView.image = nil
    }
}
