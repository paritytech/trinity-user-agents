import Foundation
import os
import SubstrateSdk
import Testing
@testable import polkadot_app

struct NonPrivateSigningFallbackTests {
    private let facade = UserDataStorageTestFacade()
    private let peer = Data(repeating: 0x0A, count: 32)
    private let ownSignKeyId = "//wallet//main"
    private let prompter = FakeConsentPrompter()
    private let pendingMessages = FakePendingMessages()
    private let signManager = ChatSignerManager(entropyManager: FixedRootEntropyManager())

    @Test func signsWithIdentityWithoutPromptWhenConsentIsStored() async throws {
        try await consentStore().grant(for: peer)

        let signer = try await makeFallback().identitySigner(for: peer, ownSignKeyId: ownSignKeyId)

        #expect(try signer.accountId == identityAccountId())
        #expect(prompter.subjects.isEmpty)
    }

    @Test func grantedPromptPersistsConsent() async throws {
        prompter.answers = [true]

        let signer = try await makeFallback().identitySigner(for: peer, ownSignKeyId: ownSignKeyId)

        #expect(try signer.accountId == identityAccountId())
        #expect(try await consentStore().hasConsent(for: peer))
        #expect(prompter.subjects.map(\.contactAccountId) == [peer])
    }

    @Test func declinedPromptFailsPendingMessagesAndStoresNothing() async throws {
        prompter.answers = [false]

        await #expect(throws: ChatSigningConsentError.declined) {
            try await makeFallback().identitySigner(for: peer, ownSignKeyId: ownSignKeyId)
        }

        #expect(pendingMessages.failedContacts == [peer])
        #expect(try await !consentStore().hasConsent(for: peer))
    }

    @Test func afterDeclineOnlyANewMessagePromptsAgain() async throws {
        prompter.answers = [false, true]
        let fallback = makeFallback()
        _ = try? await fallback.identitySigner(for: peer, ownSignKeyId: ownSignKeyId)

        pendingMessages.hasPending = false
        await #expect(throws: ChatSigningConsentError.declined) {
            try await fallback.identitySigner(for: peer, ownSignKeyId: ownSignKeyId)
        }
        #expect(prompter.subjects.count == 1)

        pendingMessages.hasPending = true
        _ = try await fallback.identitySigner(for: peer, ownSignKeyId: ownSignKeyId)
        #expect(prompter.subjects.count == 2)
    }

    @Test func promptNamesTheContact() async throws {
        try await ChatContactsFixture(facade: facade).insertContact(peer)
        prompter.answers = [true]

        _ = try await makeFallback().identitySigner(for: peer, ownSignKeyId: ownSignKeyId)

        #expect(prompter.subjects.map(\.username) == ["peer"])
    }
}

private extension NonPrivateSigningFallbackTests {
    func consentStore() -> ChatSigningConsentStore {
        ChatSigningConsentStore(storageFacade: facade, dateProvider: MockDateProvider())
    }

    func identityAccountId() throws -> AccountId {
        try signManager.makeSigner(for: ownSignKeyId).accountId
    }

    func makeFallback() -> NonPrivateSigningFallback {
        NonPrivateSigningFallback(
            consentStore: consentStore(),
            prompter: prompter,
            pendingMessages: pendingMessages,
            contactsStorageService: ContactsLocalStorageService(storageFacade: facade, logger: MockLogger()),
            signManager: signManager
        )
    }
}

private final class FakeConsentPrompter: ChatSigningConsentPrompting, @unchecked Sendable {
    private struct State {
        var answers: [Bool] = []
        var subjects: [ChatSigningConsentSubject] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var answers: [Bool] {
        get { state.withLock { $0.answers } }
        set { state.withLock { $0.answers = newValue } }
    }

    var subjects: [ChatSigningConsentSubject] { state.withLock { $0.subjects } }

    func requestConsent(for subject: ChatSigningConsentSubject) async -> Bool {
        state.withLock { state in
            state.subjects.append(subject)
            return state.answers.isEmpty ? false : state.answers.removeFirst()
        }
    }
}

private final class FakePendingMessages: ChatPendingMessagesServicing, @unchecked Sendable {
    private struct State {
        var hasPending = true
        var failedContacts: [AccountId] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var hasPending: Bool {
        get { state.withLock { $0.hasPending } }
        set { state.withLock { $0.hasPending = newValue } }
    }

    var failedContacts: [AccountId] { state.withLock { $0.failedContacts } }

    func hasPendingMessages(to _: AccountId) async throws -> Bool {
        hasPending
    }

    func failPendingMessages(to contactAccountId: AccountId) async throws {
        state.withLock { $0.failedContacts.append(contactAccountId) }
    }
}
