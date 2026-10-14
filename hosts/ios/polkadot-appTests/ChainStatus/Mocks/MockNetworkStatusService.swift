import Foundation
import AsyncExtensions
import ChainRegistry
@testable import polkadot_app

final class MockNetworkStatusService: NetworkStatusProviding {
    private let subject = AsyncCurrentValueSubject<NetworkStatus>(.connecting)

    func statusStream(for _: Set<ChainModel.Id>) -> AnyAsyncSequence<NetworkStatus> {
        subject.eraseToAnyAsyncSequence()
    }

    func simulateStatus(_ status: NetworkStatus) {
        subject.send(status)
    }
}
