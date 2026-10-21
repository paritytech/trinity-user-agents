import Foundation
import TrUAPIHost

/// The username directory that products use: the search of the chat contact search. The product learns usernames
/// only, never an account.
final class AppUsernamesHostBridge: UsernamesHostBridge, @unchecked Sendable {
    /// The most usernames that one search gives.
    static let searchLimit = 20

    private let directory: RemoteContactOperationMaking

    init(directory: RemoteContactOperationMaking) {
        self.directory = directory
    }

    func searchUsernames(productId _: String, prefix: String) async throws -> [String] {
        var seen = Set<String>()
        let usernames = try await directory.search(by: prefix)
            .map(\.username)
            .filter { seen.insert($0.lowercased()).inserted }
        return Array(usernames.prefix(Self.searchLimit))
    }
}
