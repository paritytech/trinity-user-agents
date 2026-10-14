import Foundation
import Operation_iOS

public protocol AnyCancellableCleaning {
    func clear(cancellable: inout CancellableCall?)
}

public extension AnyCancellableCleaning {
    func clear(cancellable: inout CancellableCall?) {
        let copy = cancellable
        cancellable = nil
        copy?.cancel()
    }
}
