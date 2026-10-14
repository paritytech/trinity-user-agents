import Foundation
import CoreData

extension NSManagedObjectContext {
    func first<T: NSManagedObject>(for predicate: NSPredicate?) throws -> T? {
        let request = T.fetchRequest()
        request.fetchLimit = 1
        request.predicate = predicate

        return try fetch(request).first as? T
    }
}
