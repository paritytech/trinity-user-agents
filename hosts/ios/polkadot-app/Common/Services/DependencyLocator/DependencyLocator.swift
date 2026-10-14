import Foundation

protocol DependencyLocator: AnyObject {
    func getDependency<T>() -> T?
    func setDependency(_ dependency: some Any)
}
