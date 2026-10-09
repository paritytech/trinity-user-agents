import Foundation
import AsyncExtensions

@testable import polkadot_app

/// Publishes a fixed list of paired hosts once.
final class StubPolkadotSignInHostDataProviderFactory: PolkadotSignInHostDataProviderMaking {
    private let hosts: [PolkadotSignInHost]

    init(hosts: [PolkadotSignInHost]) {
        self.hosts = hosts
    }

    func subscribeHostsSnapshot(
        deliverOn _: DispatchQueue,
        update: @escaping ([PolkadotSignInHost]) -> Void,
        failure _: @escaping (Error) -> Void
    ) -> AnyObject {
        update(hosts)
        return NSObject()
    }

    func subscribeHosts() -> AnyAsyncSequence<[PolkadotSignInHost]> {
        AsyncStream { [hosts] continuation in
            continuation.yield(hosts)
            continuation.finish()
        }.eraseToAnyAsyncSequence()
    }
}
