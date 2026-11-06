import Foundation
import PolkadotUI
import TrUAPIHost

protocol PublicProfileAvatarViewModelMaking {
    /// With `refresh`, display the cached avatar before forcing a metadata refresh.
    func createViewModel(for username: String, refresh: Bool) -> any ImageViewModelProtocol
}

final class PublicProfileAvatarViewModelFactory: PublicProfileAvatarViewModelMaking {
    private let resolveProfile: @Sendable (String, Bool) async throws -> PublicProfile
    private let loadAvatar: @Sendable (ProfileAvatar) async throws -> Data

    init(
        resolveProfile: @escaping @Sendable (String, Bool) async throws -> PublicProfile,
        loadAvatar: @escaping @Sendable (ProfileAvatar) async throws -> Data
    ) {
        self.resolveProfile = resolveProfile
        self.loadAvatar = loadAvatar
    }

    convenience init(runtimeProvider: TrUAPIHostRuntimeProviding) {
        self.init(resolveProfile: { username, force in
            try await runtimeProvider.sharedRuntime().resolvePublicProfile(username: username, force: force)
        }, loadAvatar: { avatar in
            try await runtimeProvider.sharedRuntime().loadImage(
                reference: .bulletin(cid: avatar.cid, declaredFormat: avatar.format)
            ).bytes
        })
    }

    func createViewModel(for username: String, refresh: Bool) -> any ImageViewModelProtocol {
        PublicProfileAvatarViewModel(
            username: username,
            refresh: refresh,
            resolveProfile: resolveProfile,
            loadAvatar: loadAvatar
        )
    }
}
