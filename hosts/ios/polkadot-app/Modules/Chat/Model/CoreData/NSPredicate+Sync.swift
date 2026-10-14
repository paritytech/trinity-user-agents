import Foundation

extension NSPredicate {
    static func contactsAcceptedAfter(_ date: Date) -> NSPredicate {
        NSPredicate(
            format: "%K > %@",
            #keyPath(CDChatContact.acceptedAt),
            date as NSDate
        )
    }

    static var acceptedContacts: NSPredicate {
        NSPredicate(
            format: "%K == nil",
            #keyPath(CDChatContact.chatRequest)
        )
    }

    static func removedChatsAfter(_ date: Date) -> NSPredicate {
        NSPredicate(
            format: "%K > %@",
            #keyPath(CDRemovedChat.removedAt),
            date as NSDate
        )
    }

    static func devicesCreatedAfter(_ date: Date) -> NSPredicate {
        NSPredicate(
            format: "%K > %@",
            #keyPath(CDLocalDevice.createdAt),
            date as NSDate
        )
    }

    static func messagesModifiedAfter(_ timestamp: UInt64) -> NSPredicate {
        let rawTimestamp = Int64(bitPattern: timestamp)

        return NSPredicate(
            format: "(%K > %lld) OR ((%K == nil OR %K == 0) AND %K > %lld)",
            #keyPath(CDChatMessage.modifiedAt),
            rawTimestamp,
            #keyPath(CDChatMessage.modifiedAt),
            #keyPath(CDChatMessage.modifiedAt),
            #keyPath(CDChatMessage.timestamp),
            rawTimestamp
        )
    }

    static func syncableMessagesForAcceptedContacts(
        since timestamp: UInt64?,
        ownSignKeyIds signKeyIds: Set<String>
    ) -> NSPredicate {
        let ownSignKeyPredicate = NSCompoundPredicate(orPredicateWithSubpredicates: [
            NSPredicate(
                format: "%K IN %@",
                #keyPath(CDChatMessage.chat.contact.ownSignKeyId),
                Array(signKeyIds)
            ),
            NSPredicate(
                format: "%K == nil",
                #keyPath(CDChatMessage.chat.contact.ownSignKeyId)
            )
        ])
        return NSCompoundPredicate(andPredicateWithSubpredicates: [
            messagesForAcceptedContacts(since: timestamp),
            ownSignKeyPredicate
        ])
    }
}

private extension NSPredicate {
    // Include old messages for contacts that became accepted after the checkpoint.
    static func messagesForAcceptedContacts(since timestamp: UInt64?) -> NSPredicate {
        let acceptedContact = NSPredicate(
            format: "%K == nil",
            #keyPath(CDChatMessage.chat.contact.chatRequest)
        )

        guard let timestamp else {
            return acceptedContact
        }

        let messageAddedAfter = messagesModifiedAfter(timestamp)
        let contactAcceptedAfter = NSPredicate(
            format: "%K > %@",
            #keyPath(CDChatMessage.chat.contact.acceptedAt),
            Date.fromChatTimestamp(timestamp) as NSDate
        )

        return NSCompoundPredicate(andPredicateWithSubpredicates: [
            acceptedContact,
            NSCompoundPredicate(orPredicateWithSubpredicates: [
                messageAddedAfter,
                contactAcceptedAfter
            ])
        ])
    }
}
