import Foundation
import SubstrateSdk

protocol RemoteContactResolving {
    func fetch(by accountId: AccountId) async throws -> Chat.RemoteContact?
}

final class CompoundRemoteContactResolver {
    let resolvers: [RemoteContactResolving]
    let logger: LoggerProtocol

    init(resolvers: [RemoteContactResolving], logger: LoggerProtocol) {
        self.resolvers = resolvers
        self.logger = logger
    }
}

extension CompoundRemoteContactResolver: RemoteContactResolving {
    func fetch(by accountId: AccountId) async throws -> Chat.RemoteContact? {
        for resolver in resolvers {
            do {
                if let contact = try await resolver.fetch(by: accountId) {
                    return contact
                }
            } catch {
                logger.error("Resolver failed: \(error). Skipping")
            }
        }

        return nil
    }
}
