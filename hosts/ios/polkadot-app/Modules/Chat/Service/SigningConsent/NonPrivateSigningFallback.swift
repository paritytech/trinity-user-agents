import Foundation
import os
import StatementStore
import SubstrateSdk

enum ChatSigningConsentError: Error {
    case declined
}

protocol NonPrivateSigningFallbacking: Sendable {
    /// The identity signer for a contact once the user consented, asking first if needed.
    /// - Throws: `ChatSigningConsentError.declined` after the contact's pending messages were marked failed.
    func identitySigner(for contactAccountId: AccountId, ownSignKeyId: String) async throws -> StatementStoreSigning
}

final class NonPrivateSigningFallback: @unchecked Sendable {
    private let consentStore: ChatSigningConsentStoring
    private let prompter: ChatSigningConsentPrompting
    private let pendingMessages: ChatPendingMessagesServicing
    private let contactsStorageService: ContactsLocalStorageServicing
    private let signManager: StatementStoreSignerManaging
    private let declinedContacts = OSAllocatedUnfairLock(initialState: Set<AccountId>())

    init(
        consentStore: ChatSigningConsentStoring,
        prompter: ChatSigningConsentPrompting,
        pendingMessages: ChatPendingMessagesServicing,
        contactsStorageService: ContactsLocalStorageServicing,
        signManager: StatementStoreSignerManaging
    ) {
        self.consentStore = consentStore
        self.prompter = prompter
        self.pendingMessages = pendingMessages
        self.contactsStorageService = contactsStorageService
        self.signManager = signManager
    }
}

extension NonPrivateSigningFallback: NonPrivateSigningFallbacking {
    func identitySigner(for contactAccountId: AccountId, ownSignKeyId: String) async throws -> StatementStoreSigning {
        if try await !consentStore.hasConsent(for: contactAccountId) {
            try await askConsent(for: contactAccountId)
        }

        return try signManager.makeSigner(for: ownSignKeyId)
    }
}

private extension NonPrivateSigningFallback {
    func askConsent(for contactAccountId: AccountId) async throws {
        // Batches queued before "Not now" must not prompt again; the user's next message does.
        if declinedContacts.withLock({ $0.contains(contactAccountId) }) {
            guard try await pendingMessages.hasPendingMessages(to: contactAccountId) else {
                throw ChatSigningConsentError.declined
            }
        }

        let username = try? await contactsStorageService.getContact(by: contactAccountId).asyncExecute()?.username
        let granted = await prompter.requestConsent(
            for: ChatSigningConsentSubject(contactAccountId: contactAccountId, username: username)
        )

        guard granted else {
            declinedContacts.withLock { _ = $0.insert(contactAccountId) }
            try await pendingMessages.failPendingMessages(to: contactAccountId)
            throw ChatSigningConsentError.declined
        }

        declinedContacts.withLock { _ = $0.remove(contactAccountId) }
        try await consentStore.grant(for: contactAccountId)
    }
}

extension NonPrivateSigningFallback {
    static func makeDefault() -> NonPrivateSigningFallback {
        NonPrivateSigningFallback(
            consentStore: ChatSigningConsentStore(),
            prompter: ChatSigningConsentPrompter(presenter: ChatSigningConsentAlertPresenter()),
            pendingMessages: ChatPendingMessagesService(),
            contactsStorageService: ContactsLocalStorageService(),
            signManager: ChatSignerManager()
        )
    }
}
