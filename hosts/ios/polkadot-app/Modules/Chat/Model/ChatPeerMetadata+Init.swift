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
            let policy = roomPolicy ?? ChatRoomPolicy(footer: roomFooter, declarations: nil)

            if let contact = roomContact {
                return contact.peerMetadata(name: listMetadata.name, policy: policy)
            }

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
            let name = roomMetadata?.name ?? extMetadata.name

            if let contact = roomContact {
                return Chat.PeerListMetadata(name: contact.name(fallback: name), icon: .image(nil))
            }

            return Chat.PeerListMetadata(
                name: name,
                icon: roomMetadata?.inlineIcon ?? extMetadata.icon
            )
        }
    }

    func chatWithPeerMetadata(using registry: ChatExtensionsRegistering) -> ChatWithPeerMetadata {
        ChatWithPeerMetadata(chat: self, peerMetadata: peerListMetadata(using: registry))
    }

    func chatMetadata(using registry: ChatExtensionsRegistering) -> ChatMetadata {
        let peerMetadata = peerMetadata(using: registry)
        let state: ChatMetadata.State = roomContact?.state == .invite ? .pending : .created

        return ChatMetadata(chatId: chatId, peerMetadata: peerMetadata, state: state, roomPolicy: roomPolicy)
    }
}

extension Chat.RoomContact {
    /// The header, the menu and the footer of the native person chat with this contact
    /// (`Chat.Contact.toPeerMetadata()`): no menu while a request is open or the contact is blocked, and the call
    /// buttons that the room declares while both sides accepted.
    func peerMetadata(name: String, policy: ChatRoomPolicy) -> Chat.PeerMetadata {
        let moreActions: [Chat.PeerAction] =
            switch state {
            case .invite,
                 .outgoing,
                 .incoming,
                 .blocked:
                []
            case .established,
                 .peerLeft:
                policy.toolbar + [.blockUser, .leaveChat]
            }

        return Chat.PeerMetadata(
            name: name,
            icon: .image(nil),
            input: metadataInput(policy: policy),
            moreActions: moreActions
        )
    }

    private func metadataInput(policy: ChatRoomPolicy) -> Chat.PeerMetadataInput {
        switch state {
        case .blocked:
            .blockedUser
        case .incoming:
            .incomingRequest
        case .invite,
             .outgoing:
            .outgoingRequest
        case .established,
             .peerLeft:
            // The native person chat has the payment button for every contact without a request
            // (`Chat.Contact.deriveMetadataInput()`), and the transfer goes to the account of the contact. The
            // attachment button needs the HOP attachments of the room.
            .inputField(.init(canPay: true, canAttachFile: policy.sendsMedia))
        }
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
