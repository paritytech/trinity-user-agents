import Foundation

extension Chat {
    enum Action {
        case customMessage(actionId: String, payload: Any?, messageId: String)
    }
}
