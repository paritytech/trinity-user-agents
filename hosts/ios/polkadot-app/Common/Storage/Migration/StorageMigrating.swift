import Foundation

protocol StorageMigrating {
    func requiresMigration() -> Bool
    func migrate(_ completion: @escaping () -> Void)
}
