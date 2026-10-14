import Foundation
import SubstrateSdk
import SDKLogger

public protocol BaseObservableStateStoreProtocol {
    associatedtype RemoteState: Equatable

    func add(
        observer: AnyObject,
        sendStateOnSubscription: Bool,
        queue: DispatchQueue?,
        closure: @escaping Observable<RemoteState?>.StateChangeClosure
    )
    func remove(observer: AnyObject)
    func reset()
}

open class BaseObservableStateStore<T: Equatable> {
    public typealias RemoteState = T

    public var stateObservable: Observable<T?> = .init(state: nil)
    public let logger: SDKLoggerProtocol
    public let mutex = NSLock()

    public var currentState: T? {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return stateObservable.state
    }

    public init(logger: SDKLoggerProtocol) {
        self.logger = logger
    }
}

extension BaseObservableStateStore: BaseObservableStateStoreProtocol {
    public func add(
        observer: AnyObject,
        sendStateOnSubscription: Bool,
        queue: DispatchQueue?,
        closure: @escaping Observable<T?>.StateChangeClosure
    ) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        stateObservable.addObserver(
            with: observer,
            sendStateOnSubscription: sendStateOnSubscription,
            queue: queue,
            closure: closure
        )
    }

    public func add(
        observer: AnyObject,
        queue: DispatchQueue?,
        closure: @escaping Observable<T?>.StateChangeClosure
    ) {
        add(
            observer: observer,
            sendStateOnSubscription: true,
            queue: queue,
            closure: closure
        )
    }

    public func remove(observer: AnyObject) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        stateObservable.removeObserver(by: observer)
    }

    public func reset() {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        stateObservable = .init(state: nil)
    }
}
