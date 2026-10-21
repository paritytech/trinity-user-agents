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
            let policy = roomPolicy ?? ChatRoomPolicy(footer: roomFooter)

            return Chat.PeerMetadata(
                name: listMetadata.name,
                icon: listMetadata.icon,
                input: policy.footer.showsInput ? botMetadata.input : .empty,
                moreActions: policy.toolbar + botMetadata.moreActions
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
                icon: roomDeclarations?.avatar.map { .roomImage($0) } ?? roomMetadata?.roomIcon ?? extMetadata.icon
            )
        }
    }

    func chatWithPeerMetadata(using registry: ChatExtensionsRegistering) -> ChatWithPeerMetadata {
        ChatWithPeerMetadata(chat: self, peerMetadata: peerListMetadata(using: registry))
    }

    func chatMetadata(using registry: ChatExtensionsRegistering) -> ChatMetadata {
        let peerMetadata = peerMetadata(using: registry)

        return ChatMetadata(
            chatId: chatId,
            peerMetadata: peerMetadata,
            state: .created,
            subtitle: roomDeclarations?.description,
            roomPolicy: roomPolicy
        )
    }
}

private extension Chat.RoomMetadata {
    /// An inline image draws at once. An `https` icon loads through the chat resource resolver.
    var roomIcon: Chat.PeerMetadata.Icon? {
        guard let icon else { return nil }

        switch ChatResource(source: icon) {
        case let .inline(_, data): return .image(data)
        case .address: return .resource(icon)
        case .preimage,
             .sealed,
             .hop,
             nil: return nil
        }
    }
}
