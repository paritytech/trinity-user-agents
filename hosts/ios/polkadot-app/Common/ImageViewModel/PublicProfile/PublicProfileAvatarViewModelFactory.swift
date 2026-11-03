import Foundation
import PolkadotUI

protocol PublicProfileAvatarViewModelMaking {
    /// With `refresh`, the view model's first load rereads the profile and replaces the cached avatar.
    func createViewModel(for username: String, refresh: Bool) -> any ImageViewModelProtocol
}

final class PublicProfileAvatarViewModelFactory: PublicProfileAvatarViewModelMaking {
    private let resolveAvatar: @Sendable (String, Bool) async throws -> Data?

    init(resolveAvatar: @escaping @Sendable (String, Bool) async throws -> Data?) {
        self.resolveAvatar = resolveAvatar
    }

    convenience init(runtimeProvider: TrUAPIHostRuntimeProviding) {
        self.init { username, force in
            try await runtimeProvider.sharedRuntime().loadImage(
                reference: .username(username: username, force: force)
            ).bytes
        }
    }

    func createViewModel(for username: String, refresh: Bool) -> any ImageViewModelProtocol {
        let cached = LocalImageViewModel(
            provider: PublicProfileAvatarImageDataProvider(username: username, resolveAvatar: resolveAvatar),
            optionsFactory: PublicProfileAvatarImageOptionsFactory(),
            keepsCurrentImageWhileLoading: false
        )
        guard refresh else {
            return cached
        }

        return PublicProfileAvatarRefreshViewModel(
            provider: PublicProfileAvatarImageDataProvider(
                username: username,
                force: true,
                resolveAvatar: resolveAvatar
            ),
            optionsFactory: PublicProfileAvatarImageOptionsFactory(),
            cached: cached
        )
    }
}
