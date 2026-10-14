import Foundation

extension NSPredicate {
    static func filterStorageItemsBy(identifier: String) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDChainStorageItem.identifier), identifier)
    }
}
