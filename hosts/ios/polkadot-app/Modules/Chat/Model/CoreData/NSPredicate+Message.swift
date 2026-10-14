import Foundation
import SubstrateSdk

extension NSPredicate {
    static func localMessages(from chatId: Chat.Id) -> NSPredicate {
        NSPredicate(
            format: "%K == %@",
            #keyPath(CDChatMessage.chat.identifier),
            chatId.rawRepresentation
        )
    }

    static func messages(in chat: CDChat) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDChatMessage.chat), chat)
    }

    static func messages(withIds ids: [String]) -> NSPredicate {
        NSPredicate(
            format: "%K IN %@",
            #keyPath(CDChatMessage.messageId),
            ids
        )
    }

    static func newOutgoingRemoteMessages() -> NSPredicate {
        let statusPredicate = byStatus(Chat.LocalMessage.Status.outgoing(.new))
        let chatTypePredicate = messageByChatType(.person)

        return NSCompoundPredicate(andPredicateWithSubpredicates: [statusPredicate, chatTypePredicate])
    }

    static func newLocalDeviceOutgoingRemoteRichTextMessages() -> NSPredicate {
        let statusPredicate = byStatus(Chat.LocalMessage.Status.outgoing(.new))
        let chatTypePredicate = messageByChatType(.person)
        let contentTypePredicate = messageByContentType(.richText)
        let creationSourcePredicate = byCreationSource(.localDevice)

        let predicates = [
            statusPredicate,
            chatTypePredicate,
            contentTypePredicate,
            creationSourcePredicate
        ]

        return NSCompoundPredicate(andPredicateWithSubpredicates: predicates)
    }

    static func incomingCoinageSendMessages() -> NSPredicate {
        let incomingNewPredicate = byStatus(Chat.LocalMessage.Status.incoming(.new))
        let incomingSeenPredicate = byStatus(Chat.LocalMessage.Status.incoming(.seen))
        let statusPredicate = NSCompoundPredicate(orPredicateWithSubpredicates: [
            incomingNewPredicate,
            incomingSeenPredicate
        ])

        let chatTypePredicate = messageByChatType(.person)
        let contentTypePredicate = messageByContentType(.coinageSend)

        return NSCompoundPredicate(andPredicateWithSubpredicates: [
            statusPredicate,
            chatTypePredicate,
            contentTypePredicate,
            withoutTerminalTransferState(#keyPath(CDChatMessage.incomingTransferState))
        ])
    }

    static func outgoingLocalDeviceCoinageSendMessages() -> NSPredicate {
        let outgoingNewPredicate = byStatus(Chat.LocalMessage.Status.outgoing(.new))
        let outgoingSentPredicate = byStatus(Chat.LocalMessage.Status.outgoing(.sent))
        let outgoingDeliveredPredicate = byStatus(Chat.LocalMessage.Status.outgoing(.delivered))
        let statusPredicate = NSCompoundPredicate(orPredicateWithSubpredicates: [
            outgoingNewPredicate,
            outgoingSentPredicate,
            outgoingDeliveredPredicate
        ])

        let chatTypePredicate = messageByChatType(.person)
        let contentTypePredicate = messageByContentType(.coinageSend)
        let creationSourcePredicate = byCreationSource(.localDevice)

        return NSCompoundPredicate(andPredicateWithSubpredicates: [
            statusPredicate,
            chatTypePredicate,
            contentTypePredicate,
            creationSourcePredicate,
            withoutTerminalTransferState(#keyPath(CDChatMessage.outgoingTransferState))
        ])
    }

    static func incomingRemoteRichTextMessages() -> NSPredicate {
        let incomingNewPredicate = byStatus(Chat.LocalMessage.Status.incoming(.new))
        let incomingSeenPredicate = byStatus(Chat.LocalMessage.Status.incoming(.seen))
        let statusPredicate = NSCompoundPredicate(orPredicateWithSubpredicates: [
            incomingNewPredicate,
            incomingSeenPredicate
        ])

        let chatTypePredicate = messageByChatType(.person)
        let contentTypePredicate = messageByContentType(.richText)

        let predicates = [statusPredicate, chatTypePredicate, contentTypePredicate]

        return NSCompoundPredicate(andPredicateWithSubpredicates: predicates)
    }

    static func incomingCompactedMessages() -> NSPredicate {
        let incomingNewPredicate = byStatus(Chat.LocalMessage.Status.incoming(.new))
        let incomingSeenPredicate = byStatus(Chat.LocalMessage.Status.incoming(.seen))
        let statusPredicate = NSCompoundPredicate(orPredicateWithSubpredicates: [
            incomingNewPredicate,
            incomingSeenPredicate
        ])

        let chatTypePredicate = messageByChatType(.person)
        let contentTypePredicate = messageByContentType(.compactedMessages)
        let notExpandedPredicate = NSPredicate(
            format: "%K == NO",
            #keyPath(CDChatMessage.contentExpanded)
        )

        return NSCompoundPredicate(andPredicateWithSubpredicates: [
            statusPredicate,
            chatTypePredicate,
            contentTypePredicate,
            notExpandedPredicate
        ])
    }

    static func newOutgoingChatRequestMessages() -> NSPredicate {
        let statusPredicate = byStatus(Chat.LocalMessage.Status.outgoing(.new))
        let chatTypePredicate = messageByChatType(.person)
        let contentTypePredicate = NSCompoundPredicate(orPredicateWithSubpredicates: [
            messageByContentType(.chatRequest),
            messageByContentType(.versionedChatRequest)
        ])

        let predicates = [statusPredicate, chatTypePredicate, contentTypePredicate]
        return NSCompoundPredicate(andPredicateWithSubpredicates: predicates)
    }

    static func newIncomingMessages(from chatId: Chat.Id) -> NSPredicate {
        let byStatus = byStatus(Chat.LocalMessage.Status.incoming(.new))
        let byContactId = localMessages(from: chatId)

        return NSCompoundPredicate(andPredicateWithSubpredicates: [byContactId, byStatus])
    }

    /// Messages compacted into a blob are excluded: their delivery is tracked
    /// through the compacted parent, so they must not re-enter sync logic.
    static func sentLocalDeviceMessages(to chatId: Chat.Id) -> NSPredicate {
        let byStatus = byStatus(Chat.LocalMessage.Status.outgoing(.sent))
        let byContactId = localMessages(from: chatId)
        let byCreationSource = byCreationSource(.localDevice)
        let withoutCompaction = withoutCompaction()

        return NSCompoundPredicate(andPredicateWithSubpredicates: [
            byContactId,
            byStatus,
            byCreationSource,
            withoutCompaction
        ])
    }

    static func withoutCompaction() -> NSPredicate {
        NSPredicate(
            format: "%K == nil",
            #keyPath(CDChatMessage.compactionId)
        )
    }

    static func byStatus(_ status: Chat.LocalMessage.Status) -> NSPredicate {
        NSPredicate(
            format: "%K == %d",
            #keyPath(CDChatMessage.status),
            status.rawValue
        )
    }

    static func byCreationSource(_ creationSource: Chat.LocalMessage.CreationSource) -> NSPredicate {
        NSPredicate(
            format: "%K == %d",
            #keyPath(CDChatMessage.creationSource),
            creationSource.rawValue
        )
    }

    static func messageByChatType(_ chatType: Chat.Id.ChatType) -> NSPredicate {
        NSPredicate(
            format: "%K == %d",
            #keyPath(CDChatMessage.chat.chatType),
            chatType.rawValue
        )
    }

    /// A claimed or failed transfer never re-enters the monitor.
    static func withoutTerminalTransferState(_ relationship: String) -> NSPredicate {
        NSPredicate(
            format: "%K == nil OR %K < %d",
            relationship,
            "\(relationship).status",
            TransferStateStatus.firstTerminalRawValue
        )
    }

    static func messageByContentType(_ contentType: Chat.LocalMessage.Content.ContentType) -> NSPredicate {
        NSPredicate(
            format: "%K == %d",
            #keyPath(CDChatMessage.contentType),
            contentType.rawValue
        )
    }

    static func messageByContentKey(_ contentKey: String) -> NSPredicate {
        NSPredicate(
            format: "%K == %@",
            #keyPath(CDChatMessage.contentKey),
            contentKey
        )
    }

    static func chatMessage(with remoteMessageId: String) -> NSPredicate {
        NSPredicate(
            format: "%K == %@",
            #keyPath(CDChatMessage.messageId),
            remoteMessageId
        )
    }

    static func siblingGroupMessages(
        groupingId: String,
        excluding entity: CDChatMessage
    ) -> NSPredicate {
        NSPredicate(
            format: "%K == %@ AND SELF != %@",
            #keyPath(CDChatMessage.groupingId),
            groupingId,
            entity
        )
    }

    static func editHistory(for messageId: String) -> NSPredicate {
        let originalMessagePredicate = NSPredicate(
            format: "%K == %@",
            #keyPath(CDChatMessage.messageId),
            messageId
        )

        let editedContentType = Int16(Chat.LocalMessage.Content.ContentType.edited.rawValue)
        let editedMessagesPredicate = NSPredicate(
            format: "%K == %d AND %K == %@",
            #keyPath(CDChatMessage.contentType),
            editedContentType,
            #keyPath(CDChatMessage.contentKey),
            messageId
        )

        return NSCompoundPredicate(orPredicateWithSubpredicates: [originalMessagePredicate, editedMessagesPredicate])
    }

    static func messagesByCompactionId(_ compactionId: String) -> NSPredicate {
        NSPredicate(
            format: "%K == %@",
            #keyPath(CDChatMessage.compactionId),
            compactionId
        )
    }

    static func messagesByCompactionIds(_ compactionIds: Set<String>) -> NSPredicate {
        NSPredicate(
            format: "%K IN %@",
            #keyPath(CDChatMessage.compactionId),
            compactionIds
        )
    }
}
