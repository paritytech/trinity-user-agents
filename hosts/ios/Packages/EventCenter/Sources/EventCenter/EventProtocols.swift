import Foundation

public protocol EventProtocol {
    func accept(visitor: EventVisitorProtocol)
}

public protocol EventCenterProtocol {
    func notify(with event: EventProtocol)
    func add(observer: EventVisitorProtocol, dispatchIn queue: DispatchQueue?)
    func remove(observer: EventVisitorProtocol)
}

public extension EventCenterProtocol {
    func add(observer: EventVisitorProtocol) {
        add(observer: observer, dispatchIn: nil)
    }
}
