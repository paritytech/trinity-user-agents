import Foundation

extension Chat {
    enum Action {
        case customMessage(actionId: String, payload: Any?, messageId: String)
        /// A press on a button that the host draws for a product `Actions` message.
        case productButton(messageId: String, actionId: String)
    }
}
