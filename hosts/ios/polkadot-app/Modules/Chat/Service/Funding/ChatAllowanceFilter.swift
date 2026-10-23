import Foundation
import Individuality
import SubstrateSdk

/// Releases chat allowances whose contact is gone, blocked or has no established chat;
/// every other established chat keeps renewing.
final class ChatAllowanceFilter {
    private let contactsStorageService: ContactsLocalStorageServicing
    private let signerManager: ChatPeerStatementSignerManaging

    init(
        contactsStorageService: ContactsLocalStorageServicing,
        signerManager: ChatPeerStatementSignerManaging
    ) {
        self.contactsStorageService = contactsStorageService
        self.signerManager = signerManager
    }
}

extension ChatAllowanceFilter: AllowanceReleaseFiltering {
    func recordsToRelease(from records: [AllowanceRecord]) async throws -> [AllowanceRecord] {
        let chatRecords = records.filter { $0.domain == .chat }

        guard !chatRecords.isEmpty else { return [] }

        let contacts = try await contactsStorageService.getEstablishedContacts().asyncExecute()
        let neededAccounts = try Set(contacts.filter { !$0.isBlocked }.map { contact in
            try signerManager.makeSigner(with: contact.accountId).accountId
        })

        return chatRecords.filter { !neededAccounts.contains($0.accountId) }
    }
}
