import KeyDerivation
import MessageExchangeKit

struct ChatMessageExchangeModeProvider: MessageExchangeModeProviding {
    let multideviceSignKeyIds: Set<String>

    init(tld: String) {
        multideviceSignKeyIds = [WalletDerivationPath.main(for: tld)]
    }

    func mode(forSignKeyId signKeyId: String) -> MessageExchangeMode {
        multideviceSignKeyIds.contains(signKeyId) ? .multidevice : .identity
    }
}

extension MessageExchangeModeProviding {
    func mode(for ownKeyId: Chat.Contact.Own) -> MessageExchangeMode {
        mode(forSignKeyId: ownKeyId.signKeyId)
    }

    func mode(for contact: Chat.Contact) -> MessageExchangeMode {
        mode(for: contact.ownKeyId)
    }
}
