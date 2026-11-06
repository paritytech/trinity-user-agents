import Foundation
import TrUAPIHost

actor MockPublicProfileAvatarResolver {
    private(set) var profile: PublicProfile
    private var result: Result<Data, ImageError>
    private(set) var imageRequests: [ProfileAvatar] = []

    init(result: Result<Data, ImageError>, avatar: ProfileAvatar? = nil) {
        self.result = result
        profile = PublicProfile(avatar: avatar)
    }

    func loadAvatar(_ avatar: ProfileAvatar) throws -> Data {
        imageRequests.append(avatar)
        return try result.get()
    }

    func setAvatar(_ avatar: ProfileAvatar?, bytes: Data? = nil) {
        profile = PublicProfile(avatar: avatar)
        if let bytes { result = .success(bytes) }
    }
}
