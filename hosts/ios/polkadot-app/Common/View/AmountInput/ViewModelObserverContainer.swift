import Foundation

struct ViewModelObserverWrapper<Observer> where Observer: AnyObject {
    weak var observer: Observer?

    init(observer: Observer) {
        self.observer = observer
    }
}

final class ViewModelObserverContainer<Observer> where Observer: AnyObject {
    private(set) var observers: [ViewModelObserverWrapper<Observer>] = []

    func add(observer: Observer) {
        observers = observers.filter { $0.observer != nil }

        guard !observers.contains(where: { $0.observer === observer }) else {
            return
        }

        observers.append(ViewModelObserverWrapper(observer: observer))
    }

    func remove(observer: Observer) {
        observers = observers.filter { $0.observer != nil && $0.observer !== observer }
    }
}
