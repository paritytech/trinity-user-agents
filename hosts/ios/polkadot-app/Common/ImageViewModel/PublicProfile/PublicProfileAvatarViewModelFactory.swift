import Foundation
import PolkadotUI
import TrUAPIHost

protocol PublicProfileAvatarViewModelMaking {
    func createViewModel(for username: String, options: PublicProfileResolutionOptions)
        -> any IdentifiableImageViewModelProtocol
}

final class PublicProfileAvatarViewModelFactory: PublicProfileAvatarViewModelMaking {
    private let profiles: @Sendable (String, PublicProfileResolutionOptions) async throws -> AsyncThrowingStream<
        PublicProfile,
        Error
    >
    private let imageLoader: any CIDImageLoading

    init(
        profiles: @escaping @Sendable (String, PublicProfileResolutionOptions) async throws -> AsyncThrowingStream<
            PublicProfile,
            Error
        >,
        imageLoader: any CIDImageLoading
    ) {
        self.profiles = profiles
        self.imageLoader = imageLoader
    }

    convenience init(runtimeProvider: TrUAPIHostRuntimeProviding) {
        self.init(profiles: { username, options in
            let runtime = try runtimeProvider.sharedRuntime()
            try Task.checkCancellation()
            return runtime.resolvePublicProfile(username: username, options: options)
        }, imageLoader: CIDImageLoader(loadImage: { reference in
            let runtime = try runtimeProvider.sharedRuntime()
            try Task.checkCancellation()
            return try await runtime.loadImage(reference: reference).bytes
        }))
    }

    func createViewModel(
        for username: String,
        options: PublicProfileResolutionOptions
    ) -> any IdentifiableImageViewModelProtocol {
        PublicProfileAvatarViewModel(
            username: username,
            options: options,
            profiles: profiles,
            imageLoader: imageLoader
        )
    }
}
