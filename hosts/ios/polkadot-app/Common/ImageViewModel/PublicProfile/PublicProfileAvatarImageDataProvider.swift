import Foundation
import Kingfisher

struct PublicProfileAvatarImageDataProvider: ImageDataProvider {
    private let username: String
    private let force: Bool
    private let loadAvatar: @Sendable (String, Bool) async throws -> Data

    var cacheKey: String { "public-profile:\(username)" }

    init(
        username: String,
        force: Bool = false,
        loadAvatar: @escaping @Sendable (String, Bool) async throws -> Data
    ) {
        self.username = username
        self.force = force
        self.loadAvatar = loadAvatar
    }

    func data() async throws -> Data {
        try Task.checkCancellation()
        let bytes = try await loadAvatar(username, force)
        try Task.checkCancellation()
        return bytes
    }
}
