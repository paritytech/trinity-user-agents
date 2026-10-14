import Foundation
import CoreData

extension NSManagedObjectContext {
    enum InsertError: Error {
        case unknownEntity(String)
    }

    /// Inserts a new managed object, taking its entity description from *this context's* model.
    ///
    /// `NSManagedObject.init(context:)` resolves the entity through `+entity`, which is cached on
    /// the class rather than looked up per model. Once a second model is loaded into the process —
    /// parallel tests each building their own store from the same `.mom`, for instance — CoreData
    /// cannot tell the two descriptions apart:
    ///
    ///     Multiple NSEntityDescriptions claim the NSManagedObject subclass 'CDMessageContent'
    ///     so +entity is unable to disambiguate
    ///
    /// It then returns an object belonging to whichever model registered the class first. Assigning
    /// that to a relationship raises `NSInvalidArgumentException`, naming the same class as both the
    /// desired and the given type. Resolving through the context cannot pick the wrong model.
    ///
    /// Entity names match their class names across both models, which is what makes the name-based
    /// lookup exact.
    func insertNew<Entity: NSManagedObject>(_ type: Entity.Type) throws -> Entity {
        let name = String(describing: type)

        guard let description = NSEntityDescription.entity(forEntityName: name, in: self) else {
            throw InsertError.unknownEntity(name)
        }

        return Entity(entity: description, insertInto: self)
    }
}
