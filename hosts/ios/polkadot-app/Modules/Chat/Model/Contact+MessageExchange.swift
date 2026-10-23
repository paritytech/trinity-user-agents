import Foundation
import MessageExchangeKit

extension Chat.Contact.Own {
    func toMessageExchangeOwn() -> MessageExchange.Own {
        MessageExchange.Own(
            signKeyId: signKeyId,
            encryptionKeyId: encryptionKeyId,
            pin: nil
        )
    }
}
