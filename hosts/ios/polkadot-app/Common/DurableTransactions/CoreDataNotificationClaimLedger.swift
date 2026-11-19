import AsyncExtensions
import DurableTransactions
import Foundation
import Individuality
import Operation_iOS
import StructuredConcurrency

/// Records notification slot claims in the durable ledger, each in its own store transaction.
final class CoreDataNotificationClaimLedger: NotificationClaimLedger, @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol
    private let txService: any DurableTxServicing

    init(storageFacade: StorageFacadeProtocol, txService: any DurableTxServicing) {
        databaseService = storageFacade.databaseService
        self.txService = txService
    }

    func scheduleClaim(groupId: DurableTxGroupId, policy: SubmissionPolicy) async throws {
        try await databaseService.performWrite { [txService] context in
            _ = try txService.schedule(
                domain: NotificationSlotDomain.domainId,
                groupId: groupId,
                policies: [policy],
                joining: CoreDataRegistrationScope(context: context),
                onRegister: { _, _ in }
            )
        }
    }

    func claims(groupId: DurableTxGroupId) async throws -> [DurableTxEntry] {
        try await txService.getGroupEntries(domain: NotificationSlotDomain.domainId, groupId: groupId)
    }

    func subscribeClaims(groupId: DurableTxGroupId) -> AnyAsyncSequence<[DurableTxEntry]> {
        txService.subscribeGroupEntries(domain: NotificationSlotDomain.domainId, groupId: groupId)
    }
}
