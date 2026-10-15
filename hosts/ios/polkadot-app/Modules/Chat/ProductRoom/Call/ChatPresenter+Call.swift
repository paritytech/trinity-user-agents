import Foundation

extension ChatPresenter {
    /// A call button of a product room asks the product for the call (07-LATER §11.2). Gives false for a person
    /// chat, which starts its own call.
    func routesCallToProduct(_ callType: ChatCallType) -> Bool {
        guard let productRoom else { return false }
        MainActor.assumeIsolated { productRoom.requestCall(video: callType == .video) }
        return true
    }
}
