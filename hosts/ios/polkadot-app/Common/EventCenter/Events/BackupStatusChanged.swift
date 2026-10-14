import Foundation
import ChainRegistry
import EventCenter

struct BackupStatusChanged: EventProtocol {
    func accept(visitor: EventVisitorProtocol) {
        (visitor as? AppEventVisiting)?.processBackupStatusChanged(event: self)
    }
}
