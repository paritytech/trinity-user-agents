import Foundation

extension NSPredicate {
    static func filterRuntimeMetadataItemsBy(identifier: String) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDRuntimeMetadataItem.identifier), identifier)
    }
}
