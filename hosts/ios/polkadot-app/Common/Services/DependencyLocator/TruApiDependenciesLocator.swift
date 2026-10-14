import Foundation

final class TruApiDependenciesLocator: DependencyLocator {
    private let storage: NSMapTable<NSString, AnyObject> = .strongToWeakObjects()

    func getDependency<T>() -> T? {
        let key = String(describing: T.self) as NSString
        return storage.object(forKey: key) as? T
    }

    func setDependency<T>(_ dependency: T) {
        let key = String(describing: T.self) as NSString
        storage.setObject(dependency as AnyObject, forKey: key)
    }
}
