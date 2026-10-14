import AsyncExtensions
import Foundation
@testable import polkadot_app

final class MockStatementStoreStatusProvider: StatementStoreStatusProviding {
    private let subject = AsyncCurrentValueSubject<StatementStoreStatus>(.connecting)
    private(set) var startCallCount = 0

    func statusStream() -> AnyAsyncSequence<StatementStoreStatus> {
        subject.eraseToAnyAsyncSequence()
    }

    func start() {
        startCallCount += 1
    }

    func simulateStatus(_ status: StatementStoreStatus) {
        subject.send(status)
    }
}
