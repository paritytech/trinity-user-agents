import Kingfisher
import PolkadotUI
import UIKit

/// Loads the avatar past every cache the first time, then from the cache. Loads asked for while the first one
/// runs are dropped, so a view that loads its avatar on every update rereads the profile once.
final class PublicProfileAvatarRefreshViewModel {
    private enum Stage {
        case pending
        case refreshing
        case refreshed
    }

    private let provider: ImageDataProvider
    private let optionsFactory: ImageProcessingOptionsProducing
    private let cached: any ImageViewModelProtocol
    @MainActor private var stage = Stage.pending

    init(
        provider: ImageDataProvider,
        optionsFactory: ImageProcessingOptionsProducing,
        cached: any ImageViewModelProtocol
    ) {
        self.provider = provider
        self.optionsFactory = optionsFactory
        self.cached = cached
    }
}

extension PublicProfileAvatarRefreshViewModel: ImageViewModelProtocol {
    @MainActor
    func loadImage(
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated: Bool,
        completion: ((Bool) -> Void)?
    ) {
        switch stage {
        case .pending:
            stage = .refreshing
            refresh(on: imageView, settings: settings, animated: animated, completion: completion)
        case .refreshing:
            break
        case .refreshed:
            cached.loadImage(on: imageView, settings: settings, animated: animated, completion: completion)
        }
    }

    @MainActor
    func cancel(on imageView: UIImageView) {
        cached.cancel(on: imageView)
    }
}

private extension PublicProfileAvatarRefreshViewModel {
    @MainActor
    func refresh(
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated: Bool,
        completion: ((Bool) -> Void)?
    ) {
        let options = optionsFactory.options(for: settings, animated: animated) + [.forceRefresh]
        let cachedImage = KingfisherManager.shared.cache.retrieveImageInMemoryCache(
            forKey: provider.cacheKey,
            options: options
        )
        imageView.kf.setImage(with: provider, placeholder: cachedImage, options: options) { [weak self] result in
            self?.stage = .refreshed
            switch result {
            case .success:
                completion?(true)
            case .failure:
                completion?(false)
            }
        }
    }
}
