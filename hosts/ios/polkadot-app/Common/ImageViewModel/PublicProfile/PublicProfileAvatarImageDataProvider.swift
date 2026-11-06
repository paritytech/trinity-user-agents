import Foundation
import Kingfisher
import TrUAPIHost

struct PublicProfileAvatarImageDataProvider: ImageDataProvider {
    private let avatar: ProfileAvatar
    private let loadAvatar: @Sendable (ProfileAvatar) async throws -> Data

    var cacheKey: String { avatar.cid }

    init(
        avatar: ProfileAvatar,
        loadAvatar: @escaping @Sendable (ProfileAvatar) async throws -> Data
    ) {
        self.avatar = avatar
        self.loadAvatar = loadAvatar
    }

    func data() async throws -> Data {
        try Task.checkCancellation()
        let bytes = try await loadAvatar(avatar)
        try Task.checkCancellation()
        return bytes
    }
}
