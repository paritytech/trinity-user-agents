import Foundation
import SubstrateSdk

extension StorageKeyFactory {
    static var keyModuleStoragePrefixSize: Int { 32 }

    static func keyPrefixSize(for hasher: StorageHasher) -> Int {
        switch hasher {
        case .blake128,
             .twox128,
             .blake128Concat:
            16
        case .blake256,
             .twox256:
            32
        case .twox64Concat:
            8
        case .identity:
            0
        }
    }
}
