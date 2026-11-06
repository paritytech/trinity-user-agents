import Foundation
import TrUAPIHost

actor MockPublicProfileAvatarResolver {
    struct Request: Equatable {
        let username: String
        let force: Bool
    }

    private var result: Result<Data, ImageError>
    private(set) var requests: [Request] = []

    var requestedUsernames: [String] {
        requests.map(\.username)
    }

    init(result: Result<Data, ImageError>) {
        self.result = result
    }

    func loadAvatar(username: String, force: Bool) throws -> Data {
        requests.append(Request(username: username, force: force))
        return try result.get()
    }

    func setResult(_ result: Result<Data, ImageError>) {
        self.result = result
    }
}
