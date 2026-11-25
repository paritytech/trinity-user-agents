import AsyncExtensions
import Foundation

@testable import polkadot_app

final class StubConfigStream: RemoteConfigObserving {
    private let subject = AsyncCurrentValueSubject<RemoteAppConfig?>(nil)

    func remoteConfigStream() -> AnyAsyncSequence<RemoteAppConfig> {
        subject.compacted().eraseToAnyAsyncSequence()
    }

    func send(_ config: RemoteAppConfig) {
        subject.send(config)
    }
}
