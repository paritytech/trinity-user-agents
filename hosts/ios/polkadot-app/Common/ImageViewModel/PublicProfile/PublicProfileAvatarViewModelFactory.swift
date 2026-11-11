import PolkadotUI
import TrUAPIHost

protocol PublicProfileAvatarViewModelMaking {
    func createViewModel(for username: String, options: PublicProfileResolutionOptions)
        -> any IdentifiableImageViewModelProtocol
}

final class PublicProfileAvatarViewModelFactory: PublicProfileAvatarViewModelMaking {
    private let runtimeProvider: TrUAPIHostRuntimeProviding
    private let imageLoader: CIDImageLoader

    init(runtimeProvider: TrUAPIHostRuntimeProviding) {
        self.runtimeProvider = runtimeProvider
        imageLoader = CIDImageLoader(runtimeProvider: runtimeProvider)
    }

    func createViewModel(
        for username: String,
        options: PublicProfileResolutionOptions
    ) -> any IdentifiableImageViewModelProtocol {
        PublicProfileAvatarViewModel(
            username: username,
            options: options,
            runtimeProvider: runtimeProvider,
            imageLoader: imageLoader
        )
    }
}
