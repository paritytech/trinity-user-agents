import Foundation
import TrUAPIHost

actor MockPublicProfileAvatarResolver {
    struct Request: Equatable {
        let username: String
        let force: Bool
    }

    private var profile: PublicProfile
    private var result: Result<Data, ImageError>
    private(set) var requests: [Request] = []
    private(set) var imageRequests: [ProfileAvatar] = []

    var requestedUsernames: [String] { requests.map(\.username) }

    init(result: Result<Data, ImageError>, avatar: ProfileAvatar? = nil) {
        self.result = result
        profile = PublicProfile(avatar: avatar)
    }

    func resolve(username: String, force: Bool) -> PublicProfile {
        requests.append(Request(username: username, force: force))
        return profile
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
