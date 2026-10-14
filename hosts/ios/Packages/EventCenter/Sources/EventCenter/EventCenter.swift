import Foundation

public final class EventCenter {
    public static let shared = EventCenter()

    private struct ObserverWrapper {
        weak var observer: EventVisitorProtocol?
        let dispatchQueue: DispatchQueue?
    }

    private let syncQueue: DispatchQueue

    private var wrappers: [ObserverWrapper] = []

    public init(syncQueue: DispatchQueue = DispatchQueue(label: "event.center")) {
        self.syncQueue = syncQueue
    }
}

extension EventCenter: EventCenterProtocol {
    public func notify(with event: EventProtocol) {
        syncQueue.async {
            self.wrappers = self.wrappers.filter { $0.observer != nil }

            for wrapper in self.wrappers {
                guard let observer = wrapper.observer else {
                    continue
                }

                if let queue = wrapper.dispatchQueue {
                    queue.async {
                        event.accept(visitor: observer)
                    }
                } else {
                    event.accept(visitor: observer)
                }
            }
        }
    }

    public func add(observer: EventVisitorProtocol, dispatchIn queue: DispatchQueue?) {
        syncQueue.async {
            self.wrappers = self.wrappers.filter { $0.observer != nil }

            if !self.wrappers.contains(where: { $0.observer === observer }) {
                self.wrappers.append(ObserverWrapper(observer: observer, dispatchQueue: queue))
            }
        }
    }

    public func remove(observer: EventVisitorProtocol) {
        syncQueue.async {
            self.wrappers = self.wrappers.filter { $0.observer != nil && $0.observer !== observer }
        }
    }
}
