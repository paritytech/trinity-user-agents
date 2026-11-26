import Foundation
import CommonService
import UIKitExt

protocol MessageExchangeSignInHostCoordinating: AsyncApplicationServicing, AnyObject {
    func disconnectHost(byAccountId accountId: Data) async throws

    /// Anchors SSO confirmations (signing, personhood prompts) to the given
    /// view; until attached, prompts deliver denial.
    @MainActor func setPresentationView(_ view: ControllerBackedProtocol)
}
