import CoreData
import DurableTransactions
import Foundation

/// The engine's open CoreData write transaction, handed to domain stores so their rows are written in
/// the same context and commit with the engine's row.
final class CoreDataRegistrationScope: DurableTxRegistrationScope {
    let context: NSManagedObjectContext

    init(context: NSManagedObjectContext) {
        self.context = context
    }
}

/// Lets a domain react to an engine status write inside the same transaction — coinage uses it to
/// signal its coin and voucher rows so their snapshot subscribers re-emit.
protocol DurableTxRowObserving: Sendable {
    func didChangeStatus(of entity: CDDurableTx, in context: NSManagedObjectContext)
}
