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
                contactSource: extMetadata.contactSource,
                icon: roomMetadata?.inlineIcon ?? extMetadata.icon,
                input: roomFooter == .empty ? .empty : extMetadata.input,
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

private extension Chat.RoomMetadata {
    /// Only an inline image can be drawn as it is, so an `https` room icon leaves the bot's in place.
    var inlineIcon: Chat.PeerMetadata.Icon? {
        guard let icon, icon.hasPrefix("data:"), let payloadStart = icon.range(of: ";base64,") else {
            return nil
        }

        return Data(base64Encoded: String(icon[payloadStart.upperBound...])).map { .image($0) }
    }
}
