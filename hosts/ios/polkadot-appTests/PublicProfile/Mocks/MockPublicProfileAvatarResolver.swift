import Foundation
import TrUAPIHost

actor MockPublicProfileAvatarResolver {
    struct Request: Equatable {
        let username: String
        let force: Bool
    }

    private var profile: PublicProfile
    private var profileError: ImageError?
    private var result: Result<Data, ImageError>
    private(set) var requests: [Request] = []
    private(set) var imageRequests: [ProfileAvatar] = []

    var requestedUsernames: [String] { requests.map(\.username) }

    init(result: Result<Data, ImageError>, avatar: ProfileAvatar? = nil) {
        self.result = result
        profile = PublicProfile(avatar: avatar)
    }

    func resolve(username: String, force: Bool) throws -> PublicProfile {
        requests.append(Request(username: username, force: force))
        if let profileError { throw profileError }
        return profile
    }

    func failProfileLookup(_ error: ImageError) {
        profileError = error
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
