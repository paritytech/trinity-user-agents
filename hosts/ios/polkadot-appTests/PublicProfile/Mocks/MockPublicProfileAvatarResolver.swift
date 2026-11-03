import Foundation

actor MockPublicProfileAvatarResolver {
    enum Failure: Error, Equatable {
        case offline
    }

    struct Request: Equatable {
        let username: String
        let force: Bool
    }

    private var result: Result<Data?, Failure>
    private(set) var requests: [Request] = []

    var requestedUsernames: [String] {
        requests.map(\.username)
    }

    init(result: Result<Data?, Failure>) {
        self.result = result
    }

    func resolveAvatar(username: String, force: Bool) throws -> Data? {
        requests.append(Request(username: username, force: force))
        return try result.get()
    }

    func setResult(_ result: Result<Data?, Failure>) {
        self.result = result
    }
}
