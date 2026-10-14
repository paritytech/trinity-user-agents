import Foundation
import SubstrateSdk

public extension StorageKeyFactoryProtocol {
    func accountInfoKeyForId(_ identifier: Data) throws -> Data {
        try createStorageKey(
            moduleName: "System",
            storageName: "Account",
            key: identifier,
            hasher: .blake128Concat
        )
    }

    func key(from codingPath: StorageCodingPath) throws -> Data {
        try createStorageKey(moduleName: codingPath.moduleName, storageName: codingPath.itemName)
    }
}
