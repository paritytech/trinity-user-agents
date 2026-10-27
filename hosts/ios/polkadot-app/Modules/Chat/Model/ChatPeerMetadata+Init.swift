import Foundation

extension Chat.LocalModel {
    func peerMetadata(using registry: ChatExtensionsRegistering) -> Chat.PeerMetadata {
        switch peer {
        case let .person(contact):
            return contact.toPeerMetadata()
        case let .chatExtension(extId, _):
            // Name and icon come from the list metadata, so a room reached without the list's
            // wait for the bot still shows the product; input and actions need the bot.
            let listMetadata = peerListMetadata(using: registry)
            let botMetadata = registry.getChatExtensionBot(for: extId)?.peerMetadata ?? .unknown

            return Chat.PeerMetadata(
                name: listMetadata.name,
                icon: listMetadata.icon,
                input: roomFooter == .empty ? .empty : botMetadata.input,
                moreActions: botMetadata.moreActions
            )
        }
    }

    /// The row's name and icon: the room's own where it has them, else the peer's.
    func peerListMetadata(using registry: ChatExtensionsRegistering) -> Chat.PeerListMetadata {
        switch peer {
        case let .person(contact):
            return contact.toPeerMetadata().list
        case let .chatExtension(extId, _):
            let extMetadata = registry.peerListMetadata(for: extId) ?? Chat.PeerMetadata.unknown.list

            return Chat.PeerListMetadata(
                name: roomMetadata?.name ?? extMetadata.name,
                icon: roomMetadata?.inlineIcon ?? extMetadata.icon
            )
        }
    }

    func chatWithPeerMetadata(using registry: ChatExtensionsRegistering) -> ChatWithPeerMetadata {
        ChatWithPeerMetadata(chat: self, peerMetadata: peerListMetadata(using: registry))
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
