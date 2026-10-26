import Foundation
import os
import StructuredConcurrency
import SubstrateSdk

struct ChatSigningConsentSubject: Sendable {
    let contactAccountId: AccountId
    let username: String?
}

protocol ChatSigningConsentPrompting: Sendable {
    func requestConsent(for subject: ChatSigningConsentSubject) async -> Bool
}

/// Asks the user one prompt at a time; concurrent requests for the same contact share one answer.
final class ChatSigningConsentPrompter: @unchecked Sendable {
    private let presenter: ChatSigningConsentPresenting
    private let queue = SerialOperationQueue()
    private let pending = OSAllocatedUnfairLock(initialState: [AccountId: Task<Bool, Never>]())

    init(presenter: ChatSigningConsentPresenting) {
        self.presenter = presenter
    }
}

extension ChatSigningConsentPrompter: ChatSigningConsentPrompting {
    func requestConsent(for subject: ChatSigningConsentSubject) async -> Bool {
        await consentTask(for: subject).value
    }
}

extension ChatSigningConsentPrompter {
    /// Joins the open prompt for the contact or queues a new one.
    func consentTask(for subject: ChatSigningConsentSubject) -> Task<Bool, Never> {
        let contactAccountId = subject.contactAccountId

        return pending.withLock { pending in
            if let existing = pending[contactAccountId] {
                return existing
            }

            let task = Task { [queue, presenter, weak self] in
                let granted = await queue.run { await presenter.askConsent(for: subject) }
                self?.pending.withLock { _ = $0.removeValue(forKey: contactAccountId) }
                return granted
            }
            pending[contactAccountId] = task
            return task
        }
    }
}
