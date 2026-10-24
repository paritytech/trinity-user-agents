import IssueMonitoring
import UIKit

extension GameReminderEvent.AppState {
    init?(_ state: UIApplication.State) {
        switch state {
        case .active:
            self = .active
        case .inactive:
            self = .inactive
        case .background:
            self = .background
        @unknown default:
            return nil
        }
    }
}
