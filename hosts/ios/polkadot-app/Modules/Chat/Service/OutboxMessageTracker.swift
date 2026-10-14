import Foundation
import MessageExchangeKit
import SubstrateSdk

struct OutboxMessages {
    let contact: Chat.Contact
    let messagesToSend: [Chat.LocalMessage]

    func messageIds() -> Set<Chat.MessageId> {
        Set(messagesToSend.map(\.messageId))
    }
}

protocol OutboxMessageTracking {
    func setContacts(_ newContacts: [AccountId: Chat.Contact])
    func getContact(for accountId: AccountId) -> Chat.Contact?

    func insert(messages: [Chat.LocalMessage])
    var hasMessagesToSend: Bool { get }
    func prepareMessagesToSend() -> [OutboxMessages]
    func markInFlight(messageIds: Set<Chat.MessageId>)
    func insertAsInFlight(messages: [Chat.LocalMessage])
    func markFailed(messageIds: Set<Chat.MessageId>)

    @discardableResult
    func markSent(messageIds: Set<Chat.MessageId>) -> Set<Chat.MessageId>

    func remove(messageIds: Set<Chat.MessageId>)
    func clear()
}

final class OutboxMessageTracker {
    private var contacts: [AccountId: Chat.Contact] = [:]
    private var messagesToSend: [Chat.MessageId: Chat.LocalMessage] = [:]
    private var inFlightMessages: [Chat.MessageId: Chat.LocalMessage] = [:]
}

private extension OutboxMessageTracker {
    func hasMessage(with messageId: Chat.MessageId) -> Bool {
        messagesToSend[messageId] != nil || inFlightMessages[messageId] != nil
    }

    func removeMessagesToSend(forRemovedContacts removedAccountIds: Set<AccountId>) {
        messagesToSend = messagesToSend.filter { message in
            guard let accountId = message.value.contactAccountId else {
                return false
            }
            return !removedAccountIds.contains(accountId)
        }
    }

    func contactIdsWithChangedSession(
        oldContacts: [AccountId: Chat.Contact],
        newContacts: [AccountId: Chat.Contact]
    ) -> Set<AccountId> {
        newContacts.reduce(into: Set<AccountId>()) { result, item in
            let accountId = item.key
            let newContact = item.value

            guard let oldContact = oldContacts[accountId] else {
                return
            }

            let isChanged = newContact
                .toMessageExchangeSessionRequest()
                .requiresSessionRecreation(
                    comparedTo: oldContact.toMessageExchangeSessionRequest()
                )

            if isChanged {
                result.insert(accountId)
            }
        }
    }

    func updateInFlightMessages(
        removedAccountIds: Set<AccountId>,
        changedSessionContactIds: Set<AccountId>
    ) {
        for (messageId, message) in inFlightMessages {
            guard let accountId = message.contactAccountId else {
                inFlightMessages[messageId] = nil
                continue
            }

            if removedAccountIds.contains(accountId) {
                inFlightMessages[messageId] = nil
            } else if changedSessionContactIds.contains(accountId),
                      message.status == .outgoing(.new) {
                inFlightMessages[messageId] = nil
                messagesToSend[messageId] = message
            }
        }
    }
}

extension OutboxMessageTracker: OutboxMessageTracking {
    func setContacts(_ newContacts: [AccountId: Chat.Contact]) {
        let changedSessionContactIds = contactIdsWithChangedSession(
            oldContacts: contacts,
            newContacts: newContacts
        )
        let removedAccountIds = Set(contacts.keys).subtracting(Set(newContacts.keys))
        contacts = newContacts

        if !removedAccountIds.isEmpty {
            removeMessagesToSend(forRemovedContacts: removedAccountIds)
        }

        updateInFlightMessages(
            removedAccountIds: removedAccountIds,
            changedSessionContactIds: changedSessionContactIds
        )
    }

    func getContact(for accountId: AccountId) -> Chat.Contact? {
        contacts[accountId]
    }

    func insert(messages: [Chat.LocalMessage]) {
        messages.forEach { message in
            guard !hasMessage(with: message.messageId) else {
                return
            }

            messagesToSend[message.messageId] = message
        }
    }

    var hasMessagesToSend: Bool {
        !messagesToSend.isEmpty
    }

    func prepareMessagesToSend() -> [OutboxMessages] {
        let messages = messagesToSend.values.sorted { message1, message2 in
            ChatMessageComparator.timestampThenOrderComparator(
                message1: message1,
                message2: message2
            )
        }

        let messagesByPeer = messages.reduce(into: [AccountId: [Chat.LocalMessage]]()) { accum, message in
            guard let accountId = message.contactAccountId else {
                return
            }

            let peerMessages = accum[accountId] ?? []
            accum[accountId] = peerMessages + [message]
        }

        return messagesByPeer.compactMap { contactAndMessages in
            guard let contact = contacts[contactAndMessages.key] else {
                return nil
            }

            return OutboxMessages(contact: contact, messagesToSend: contactAndMessages.value)
        }
    }

    func markInFlight(messageIds: Set<Chat.MessageId>) {
        messageIds.forEach { messageId in
            inFlightMessages[messageId] = messagesToSend[messageId]
            messagesToSend[messageId] = nil
        }
    }

    func insertAsInFlight(messages: [Chat.LocalMessage]) {
        messages.forEach { message in
            inFlightMessages[message.messageId] = message
        }
    }

    func markFailed(messageIds: Set<Chat.MessageId>) {
        messageIds.forEach { messageId in
            guard let message = inFlightMessages[messageId] else {
                return
            }

            inFlightMessages[messageId] = nil
            messagesToSend[messageId] = message
        }
    }

    @discardableResult
    func markSent(messageIds: Set<Chat.MessageId>) -> Set<Chat.MessageId> {
        messageIds.reduce(into: Set()) {
            guard inFlightMessages.removeValue(forKey: $1) != nil else {
                return
            }
            $0.insert($1)
        }
    }

    func remove(messageIds: Set<Chat.MessageId>) {
        messageIds.forEach { messageId in
            inFlightMessages[messageId] = nil
            messagesToSend[messageId] = nil
        }
    }

    func clear() {
        contacts = [:]
        messagesToSend = [:]
        inFlightMessages = [:]
    }
}
