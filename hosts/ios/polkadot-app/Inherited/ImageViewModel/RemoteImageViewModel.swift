import Kingfisher
import SVGKit
import UIKit
import PolkadotUI

final class RemoteImageViewModel: NSObject {
    private let url: URL
    private let optionsFactory: ImageProcessingOptionsProducing

    init(
        url: URL,
        optionsFactory: ImageProcessingOptionsProducing = ImageProcessingOptionsFactory()
    ) {
        self.url = url
        self.optionsFactory = optionsFactory
    }
}

extension RemoteImageViewModel: ImageViewModelProtocol {
    @MainActor func loadImage(
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated: Bool,
        completion: ((Bool) -> Void)? = nil
    ) {
        let options = optionsFactory.options(for: settings, animated: animated)

        imageView.kf.setImage(
            with: url,
            options: options
        ) { result in
            switch result {
            case .success:
                completion?(true)
            case .failure:
                completion?(false)
            }
        }
    }

    @MainActor func cancel(on imageView: UIImageView) {
        imageView.kf.cancelDownloadTask() // cancel any download task

        let url: URL? = nil
        imageView.kf.setImage(with: url) // cancel any cache retrieval task
    }
}

extension RemoteImageViewModel: Identifiable {
    var identifier: String {
        url.absoluteString
    }
}
