import Foundation

extension Chat.LocalModel {
    func peerMetadata(using registry: ChatExtensionsRegistering) -> Chat.PeerMetadata {
        switch peer {
        case let .person(contact):
            return contact.toPeerMetadata()
        case let .chatExtension(extId, _):
            let extMetadata = registry.getChatExtensionBot(for: extId)?.peerMetadata ?? .unknown

            return Chat.PeerMetadata(
                name: roomMetadata?.name ?? extMetadata.name,
                icon: extMetadata.icon,
                input: extMetadata.input,
                moreActions: extMetadata.moreActions
            )
        }
    }

    func chatWithPeerMetadata(using registry: ChatExtensionsRegistering) -> ChatWithPeerMetadata {
        let peerMetadata = peerMetadata(using: registry)

        return ChatWithPeerMetadata(chat: self, peerMetadata: peerMetadata)
    }

    func chatMetadata(using registry: ChatExtensionsRegistering) -> ChatMetadata {
        let peerMetadata = peerMetadata(using: registry)

        return ChatMetadata(chatId: chatId, peerMetadata: peerMetadata, state: .created)
    }
}
