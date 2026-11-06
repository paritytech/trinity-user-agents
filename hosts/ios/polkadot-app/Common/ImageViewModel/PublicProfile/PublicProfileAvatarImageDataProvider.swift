import Foundation
import Kingfisher
import TrUAPIHost

struct PublicProfileAvatarImageDataProvider: ImageDataProvider {
    private let avatar: ProfileAvatar
    private let cache: PublicProfileAvatarImageCache
    private let loadAvatar: @Sendable (ProfileAvatar) async throws -> Data

    var cacheKey: String { "\(avatar.cid):\(avatar.format)" }

    init(
        avatar: ProfileAvatar,
        cache: PublicProfileAvatarImageCache = .shared,
        loadAvatar: @escaping @Sendable (ProfileAvatar) async throws -> Data
    ) {
        self.avatar = avatar
        self.cache = cache
        self.loadAvatar = loadAvatar
    }

    func data() async throws -> Data {
        try Task.checkCancellation()
        if let bytes = try await Task.detached(operation: { try cache.data(for: avatar) }).value {
            try Task.checkCancellation()
            return bytes
        }
        let bytes = try await loadAvatar(avatar)
        try Task.checkCancellation()
        await Task.detached {
            do {
                try cache.store(bytes, for: avatar.cid)
            } catch {
                Logger.shared.error("Failed to cache public profile image: \(error)")
            }
        }.value
        return bytes
    }
}
