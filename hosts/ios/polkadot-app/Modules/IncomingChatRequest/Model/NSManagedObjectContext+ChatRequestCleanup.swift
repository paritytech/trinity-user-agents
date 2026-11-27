import CoreData
import SubstrateSdk

extension NSManagedObjectContext {
    func deleteChatContactAndChat(for contactId: AccountId) {
        if let contact: CDChatContact = try? first(for: .contact(for: contactId)) {
            delete(contact)
        }

        if let chat: CDChat = try? first(for: .chatWithContact(for: contactId)) {
            delete(chat)
        }
    }

    /// Keeps the request row as `outgoing(.cancelled)`; returns false when it is not the contact's pending request.
    func cancelOutgoingChatRequest(_ requestId: String) throws -> Bool {
        let fetchRequest = CDChatRequest.fetchRequest()
        fetchRequest.predicate = .chatRequestById(requestId)
        fetchRequest.fetchLimit = 1

        guard
            let request = try fetch(fetchRequest).first,
            request.status == Chat.RequestStatus.outgoing(.pending).rawValue,
            request.contact?.chatRequest == request,
            let contactId = try request.contactId?.fromHex()
        else {
            return false
        }

        request.status = Chat.RequestStatus.outgoing(.cancelled).rawValue
        try deleteOutgoingStatementPublication(for: requestId)
        deleteChatContactAndChat(for: contactId)
        try saveRemovedChat(for: contactId)

        return true
    }

    func saveRemovedChat(for contactId: AccountId) throws {
        let entity: CDRemovedChat = try first(for: .removedChat(for: contactId)) ?? insertNew(CDRemovedChat.self)
        try RemovedChatMapper().populate(
            entity: entity,
            from: Chat.RemovedChat(accountId: contactId, removedAt: Date()),
            using: self
        )
    }

    func chatHasConversation(withContact contactId: AccountId) throws -> Bool {
        guard let chat: CDChat = try first(for: .chatWithContact(for: contactId)) else {
            return false
        }

        let fetchRequest = CDChatMessage.fetchRequest()
        fetchRequest.predicate = .messages(in: chat)

        return try fetch(fetchRequest).contains { message in
            let isUnknownType = Chat.LocalMessage.Content.ContentType(rawValue: UInt8(message.contentType)) == nil
            return isUnknownType || (!message.isSystem && !message.isChatHandshake)
        }
    }
}
