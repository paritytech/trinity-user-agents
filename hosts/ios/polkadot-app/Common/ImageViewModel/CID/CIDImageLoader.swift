import Kingfisher
import PolkadotUI
import TrUAPIHost
import UIKit

@MainActor
protocol CIDImageLoading: Sendable {
    func load(
        _ reference: ImageReference,
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated: Bool,
        completion: ((Bool) -> Void)?
    )

    func cancel(on imageView: UIImageView)
}

@MainActor
final class CIDImageLoader: CIDImageLoading {
    private let loadImage: @Sendable (ImageReference) async throws -> Data

    nonisolated init(loadImage: @escaping @Sendable (ImageReference) async throws -> Data) {
        self.loadImage = loadImage
    }

    func load(
        _ reference: ImageReference,
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated: Bool,
        completion: ((Bool) -> Void)?
    ) {
        let optionsFactory =
            switch reference {
            case let .bulletin(_, format): CIDImageOptionsFactory(format: format)
            }
        var options = optionsFactory.options(for: settings, animated: animated)
        options.append(.keepCurrentImageWhileLoading)
        let provider = CIDImageDataProvider(reference: reference, loadImage: loadImage)
        imageView.kf.setImage(with: provider, options: options) { result in
            switch result {
            case .success:
                completion?(true)
            case .failure(.requestError(reason: .dataProviderCancelled)):
                break
            case let .failure(error):
                if !error.isTaskCancelled, !error.isNotCurrentTask {
                    completion?(false)
                }
            }
        }
    }

    func cancel(on imageView: UIImageView) {
        imageView.kf.cancelDownloadTask()
    }
}
