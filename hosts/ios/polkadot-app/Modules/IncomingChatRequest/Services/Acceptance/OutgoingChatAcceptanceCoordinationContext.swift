import Foundation
import SubstrateSdk

/// Starts the first delivery of each new acceptance once; mirrors `OutgoingChatRequestCoordinationContext`.
actor OutgoingChatAcceptanceCoordinationContext {
    enum MessageState {
        case sending(Task<Void, Never>)
        case sent

        var isSent: Bool {
            switch self {
            case .sent:
                true
            case .sending:
                false
            }
        }
    }

    nonisolated let logger: LoggerProtocol

    init(logger: LoggerProtocol) {
        self.logger = logger
    }

    private var contactsById: [AccountId: Chat.Contact] = [:]
    private var messageState: [String: MessageState] = [:]
    private var outgoingAcceptancesTask: Task<Void, Never>?

    func update(
        contacts: [Chat.Contact],
        outgoingAcceptanceTaskBuilder: () -> Task<Void, Never>
    ) {
        let newContactsById = contacts.reduce(into: [AccountId: Chat.Contact]()) { $0[$1.accountId] = $1 }
        let contactsChanged = Set(newContactsById.keys) != Set(contactsById.keys)

        contactsById = newContactsById

        // Re-subscribing replays pending acceptances whose contact was not known on the previous pass.
        guard contactsChanged || outgoingAcceptancesTask == nil else { return }

        outgoingAcceptancesTask?.cancel()
        outgoingAcceptancesTask = outgoingAcceptanceTaskBuilder()

        messageState = messageState.filter { !$0.value.isSent }
    }

    func process(
        acceptanceMessages: [Chat.LocalMessage],
        deliver: @escaping @Sendable (OutgoingChatAcceptanceInput) async -> Void
    ) {
        for message in acceptanceMessages where messageState[message.messageId] == nil {
            guard case let .person(accountId) = message.chatId, let contact = contactsById[accountId] else {
                logger.debug("No contact yet for acceptance \(message.messageId)")
                continue
            }

            guard let input = OutgoingChatAcceptanceInput(message: message, contact: contact) else {
                logger.error("Not an acceptance message")
                continue
            }

            let task = Task { [weak self] in
                await deliver(input)
                await self?.finishDelivery(of: input.statementId)
            }

            messageState[input.statementId] = .sending(task)
        }
    }
}

extension OutgoingChatAcceptanceCoordinationContext {
    func cancelAll() {
        outgoingAcceptancesTask?.cancel()
        outgoingAcceptancesTask = nil

        for case let .sending(task) in messageState.values {
            task.cancel()
        }

        messageState = [:]
        contactsById = [:]
    }
}

private extension OutgoingChatAcceptanceCoordinationContext {
    // A cancelled delivery is forgotten, so the next run picks the still unpublished acceptance up again.
    func finishDelivery(of messageId: String) {
        messageState[messageId] = Task.isCancelled ? nil : .sent
    }
}
