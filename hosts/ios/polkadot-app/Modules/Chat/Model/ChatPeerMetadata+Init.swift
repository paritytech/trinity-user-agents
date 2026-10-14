import Foundation

extension Chat.LocalModel {
    func peerMetadata(using registry: ChatExtensionsRegistering) -> Chat.PeerMetadata {
        switch peer {
        case let .person(contact):
            return contact.toPeerMetadata()
        case let .chatExtension(extId, _):
            let extMetadata = registry.getChatExtensionBot(for: extId)?.peerMetadata ?? .unknown
            let policy = ChatRoomPolicy(footer: roomFooter)

            return Chat.PeerMetadata(
                name: roomMetadata?.name ?? extMetadata.name,
                icon: roomDeclarations?.avatar.map { .roomImage($0) } ?? roomMetadata?.roomIcon ?? extMetadata.icon,
                input: policy.footer == .empty ? .empty : extMetadata.input,
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
             nil: return nil
        }
    }
}
