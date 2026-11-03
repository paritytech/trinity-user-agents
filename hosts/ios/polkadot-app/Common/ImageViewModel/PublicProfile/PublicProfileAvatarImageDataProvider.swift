import Foundation
import Kingfisher

enum PublicProfileAvatarImageError: Error, Equatable {
    case unavailable
}

struct PublicProfileAvatarImageDataProvider: ImageDataProvider {
    private let username: String
    private let force: Bool
    private let resolveAvatar: @Sendable (String, Bool) async throws -> Data?

    var cacheKey: String { "public-profile:\(username)" }

    init(
        username: String,
        force: Bool = false,
        resolveAvatar: @escaping @Sendable (String, Bool) async throws -> Data?
    ) {
        self.username = username
        self.force = force
        self.resolveAvatar = resolveAvatar
    }

    func data() async throws -> Data {
        try Task.checkCancellation()
        guard let bytes = try await resolveAvatar(username, force) else {
            throw PublicProfileAvatarImageError.unavailable
        }
        try Task.checkCancellation()
        return bytes
    }
}
