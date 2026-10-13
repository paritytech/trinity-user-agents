import AsyncExtensions
@testable import Coinage

/// A backup status the test moves by hand; every subscriber sees the current value first.
final class StubAccountBackupObserver: CoinageAccountBackupObserving, @unchecked Sendable {
    let status: AsyncCurrentValueSubject<CoinageAccountBackupStatus>

    init(status: CoinageAccountBackupStatus = .registering) {
        self.status = AsyncCurrentValueSubject(status)
    }

    func subscribeStatus() -> AnyAsyncSequence<CoinageAccountBackupStatus> {
        status.eraseToAnyAsyncSequence()
    }
}
