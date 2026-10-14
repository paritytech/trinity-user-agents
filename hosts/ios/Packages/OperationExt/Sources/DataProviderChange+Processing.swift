import Foundation
import Operation_iOS

public extension Array {
    func reduceToLastChange<T>() -> T? where Element == DataProviderChange<T> {
        reduce(nil) { _, item in
            switch item {
            case let .insert(newItem),
                 let .update(newItem):
                newItem
            case .delete:
                nil
            }
        }
    }

    func allChangedItems<T>() -> [T] where Element == DataProviderChange<T> {
        compactMap { change in
            switch change {
            case let .insert(newItem):
                newItem
            case let .update(newItem):
                newItem
            case .delete:
                nil
            }
        }
    }

    func mergeToDict<T: Identifiable>(
        _ dict: [String: T]
    ) -> [String: T] where Element == DataProviderChange<T> {
        reduce(into: dict) { result, change in
            switch change {
            case let .insert(newItem),
                 let .update(newItem):
                result[newItem.identifier] = newItem
            case let .delete(deletedIdentifier):
                result[deletedIdentifier] = nil
            }
        }
    }
}

extension Set where Element: Operation_iOS.Identifiable {
    func allInsertChanges() -> [DataProviderChange<Element>] {
        map { DataProviderChange.insert(newItem: $0) }
    }
}
