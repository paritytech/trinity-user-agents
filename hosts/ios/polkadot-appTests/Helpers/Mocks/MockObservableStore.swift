import Foundation
import CommonService

final class MockObservableStore<T: Equatable>: BaseObservableStateStore<T>, @unchecked Sendable {
    func updateState(_ newState: T?) {
        mutex.lock()
        defer { mutex.unlock() }
        stateObservable.state = newState
    }
}
