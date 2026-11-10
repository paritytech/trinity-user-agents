import Foundation
import Operation_iOS
import SubstrateSdk

extension Chat {
    enum PeerPlatform: String {
        case android
        case ios
    }

    /// A known device belonging to a peer contact.
    struct PeerDevice: Equatable, Hashable {
        let statementAccountId: Data
        let encryptionPublicKey: Data
    }

    struct Contact: Equatable {
        struct Own: Hashable {
            let signKeyId: String
            let encryptionKeyId: String
        }

        let accountId: AccountId
        var username: String
        var publicKey: Data
        var pin: String?
        var pushId: String?
        var pushToken: Data?
        var voipPushToken: Data?
        var peerPlatform: PeerPlatform?
        var lastOwnToken: Data?
        var voipLastOwnToken: Data?
        var chatRequest: Chat.Request?
        var ownKeyId: Contact.Own
        var imageData: Data?
        var isBlocked: Bool
        var devices: [PeerDevice]
        var pendingDevicesFanOut: Bool
        var addedAt: Date?
        var acceptedAt: Date?

        var supportsVoIPPushes: Bool {
            switch peerPlatform {
            case .android,
                 nil:
                false
            case .ios:
                true
            }
        }
    }
}

extension Chat.Contact: Operation_iOS.Identifiable {
    var identifier: String {
        accountId.toHex()
    }
}

extension Chat.Contact {
    init(remoteContact: Chat.RemoteContact, ownKeyId: Own) {
        self.init(
            accountId: remoteContact.accountId,
            username: remoteContact.username,
            publicKey: remoteContact.chatPublicKey.rawData,
            ownKeyId: ownKeyId,
            imageData: remoteContact.imageData,
            isBlocked: false,
            devices: [],
            pendingDevicesFanOut: false,
            addedAt: Date(),
            acceptedAt: nil
        )
    }

    func adding(
        chatRequest: Chat.Request,
        ownKeyId: Own
    ) -> Chat.Contact {
        Chat.Contact(
            accountId: accountId,
            username: username,
            publicKey: publicKey,
            pin: pin,
            pushId: pushId,
            pushToken: pushToken,
            voipPushToken: voipPushToken,
            peerPlatform: peerPlatform,
            lastOwnToken: lastOwnToken,
            voipLastOwnToken: voipLastOwnToken,
            chatRequest: chatRequest,
            ownKeyId: ownKeyId,
            imageData: imageData,
            isBlocked: isBlocked,
            devices: devices,
            pendingDevicesFanOut: pendingDevicesFanOut,
            addedAt: addedAt,
            acceptedAt: acceptedAt
        )
    }

    func updatingLastOwnToken(_ lastOwnToken: Data) -> Chat.Contact {
        Chat.Contact(
            accountId: accountId,
            username: username,
            publicKey: publicKey,
            pin: pin,
            pushId: pushId,
            pushToken: pushToken,
            voipPushToken: voipPushToken,
            peerPlatform: peerPlatform,
            lastOwnToken: lastOwnToken,
            voipLastOwnToken: voipLastOwnToken,
            ownKeyId: ownKeyId,
            imageData: imageData,
            isBlocked: isBlocked,
            devices: devices,
            pendingDevicesFanOut: pendingDevicesFanOut,
            addedAt: addedAt,
            acceptedAt: acceptedAt
        )
    }

    func updatingVoIPLastOwnToken(_ voipLastOwnToken: Data) -> Chat.Contact {
        Chat.Contact(
            accountId: accountId,
            username: username,
            publicKey: publicKey,
            pin: pin,
            pushId: pushId,
            pushToken: pushToken,
            voipPushToken: voipPushToken,
            peerPlatform: peerPlatform,
            lastOwnToken: lastOwnToken,
            voipLastOwnToken: voipLastOwnToken,
            ownKeyId: ownKeyId,
            imageData: imageData,
            isBlocked: isBlocked,
            devices: devices,
            pendingDevicesFanOut: pendingDevicesFanOut,
            addedAt: addedAt,
            acceptedAt: acceptedAt
        )
    }

    func updatingPendingDevicesFanOut(_ pendingDevicesFanOut: Bool) -> Chat.Contact {
        Chat.Contact(
            accountId: accountId,
            username: username,
            publicKey: publicKey,
            pin: pin,
            pushId: pushId,
            pushToken: pushToken,
            voipPushToken: voipPushToken,
            peerPlatform: peerPlatform,
            lastOwnToken: lastOwnToken,
            voipLastOwnToken: voipLastOwnToken,
            chatRequest: chatRequest,
            ownKeyId: ownKeyId,
            imageData: imageData,
            isBlocked: isBlocked,
            devices: devices,
            pendingDevicesFanOut: pendingDevicesFanOut,
            addedAt: addedAt,
            acceptedAt: acceptedAt
        )
    }

    func updatingPushId(_ pushId: String) -> Chat.Contact {
        Chat.Contact(
            accountId: accountId,
            username: username,
            publicKey: publicKey,
            pin: pin,
            pushId: pushId,
            pushToken: pushToken,
            voipPushToken: voipPushToken,
            peerPlatform: peerPlatform,
            lastOwnToken: lastOwnToken,
            voipLastOwnToken: voipLastOwnToken,
            ownKeyId: ownKeyId,
            imageData: imageData,
            isBlocked: isBlocked,
            devices: devices,
            pendingDevicesFanOut: pendingDevicesFanOut,
            addedAt: addedAt,
            acceptedAt: acceptedAt
        )
    }

    var hasOutgoingChatRequest: Bool {
        chatRequest?.status.statusClass == .outgoing
    }

    var hasIncomingChatRequest: Bool {
        chatRequest?.status.statusClass == .incoming
    }

    var hasNewIncomingChatRequest: Bool {
        guard hasIncomingChatRequest else {
            return false
        }

        let hasNewMessage = chatRequest?.message?.status == .incoming(.new)

        return hasNewMessage
    }

    var isReadyForMessaging: Bool {
        !hasIncomingChatRequest && !hasOutgoingChatRequest
    }
}

extension Chat.Contact {
    func toPeerMetadata() -> Chat.PeerMetadata {
        let moreActions: [Chat.PeerAction] =
            if isBlocked || chatRequest != nil {
                []
            } else {
                [.audioCall, .videoCall, .blockUser, .leaveChat]
            }

        return Chat.PeerMetadata(
            name: username,
            icon: .image(imageData),
            input: deriveMetadataInput(),
            moreActions: moreActions
        )
    }
}

private extension Chat.Contact {
    func deriveMetadataInput() -> Chat.PeerMetadataInput {
        if isBlocked {
            return .blockedUser
        }

        guard let chatRequest else {
            return .inputField(.init(canPay: true, canAttachFile: true))
        }

        switch chatRequest.status {
        case .incoming:
            return .incomingRequest
        case .outgoing:
            return .outgoingRequest
        }
    }
}
