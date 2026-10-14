import Foundation

enum RootDependencyLocator {
    private static let locators: NSMapTable<NSString, AnyObject> = .strongToStrongObjects()

    static func getDependency<T>() -> T? {
        let key = String(describing: T.self) as NSString
        return locators.object(forKey: key) as? T
    }

    static func setDependency<T>(_ dependency: T) {
        let key = String(describing: T.self) as NSString
        locators.setObject(dependency as AnyObject, forKey: key)
    }
}
