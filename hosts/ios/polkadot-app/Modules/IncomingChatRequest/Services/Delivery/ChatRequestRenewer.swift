import Foundation
import Individuality

protocol ChatRequestRenewing: Sendable {
    func renew() async
}

final class ChatRequestRenewer: @unchecked Sendable {
    private static let runKey = "run"

    private let store: ChatRequestRenewalStoring
    private let allocator: NotificationStatementAccountAllocating
    private let signers: ChatRequestDeliverySigning
    private let outgoingService: OutgoingChatRequestServicing
    private let diagnostics: ChatRequestDiagnostics

    init(
        store: ChatRequestRenewalStoring,
        allocator: NotificationStatementAccountAllocating,
        signers: ChatRequestDeliverySigning,
        outgoingService: OutgoingChatRequestServicing,
        diagnostics: ChatRequestDiagnostics
    ) {
        self.store = store
        self.allocator = allocator
        self.signers = signers
        self.outgoingService = outgoingService
        self.diagnostics = diagnostics
    }
}

extension ChatRequestRenewer: ChatRequestRenewing {
    func renew() async {
        do {
            let candidates = try await store.renewalCandidates()
            let period = try await allocator.currentPeriod()
            let stale = candidates.filter { $0.period < period }
            let current = candidates.filter { $0.period >= period }

            diagnostics.logger.debug("Chat request renewal: \(stale.count) stale, \(current.count) of period \(period)")

            try await renewStale(stale, into: period)
            await resendMissing(current)
            diagnostics.issues.renewalFailing.recordRecovery(for: Self.runKey)
        } catch {
            diagnostics.logger.error("Chat request renewal failed: \(error)")
            diagnostics.issues.renewalFailing.recordFailure(for: Self.runKey, error: error, counters: [:])
        }
    }
}

private extension ChatRequestRenewer {
    func renewStale(_ stale: [ChatRequestRenewalCandidate], into period: UInt32) async throws {
        guard !stale.isEmpty else { return }

        let renewals = try stale.map { candidate in
            try (candidate, signers.anonymous(requestId: candidate.message.messageId, period: period))
        }

        let claimed = try await Set(allocator.initiateAllocations(for: renewals.map(\.1.signer.accountId)))
        reportIfStarved(renewals.filter { !claimed.contains($0.1.signer.accountId) }.map(\.0), period: period)

        await withTaskGroup(of: Void.self) { group in
            for (candidate, signer) in renewals where claimed.contains(signer.signer.accountId) {
                group.addTask { await self.publishOnceAllocated(candidate, from: signer) }
            }
        }
    }

    func publishOnceAllocated(_ candidate: ChatRequestRenewalCandidate, from signer: ChatRequestDeliverySigner) async {
        let requestId = candidate.message.messageId

        do {
            try await allocator.awaitAllocated(
                signer.signer.accountId,
                timeout: ChatRequestDeliveryAccountResolver.allocationTimeout
            )
        } catch {
            diagnostics.logger.error("Chat request \(requestId) slot for \(signer.period) not claimed: \(error)")
            return
        }

        do {
            try await publish(candidate, from: signer)
            try await store.updateAnonymousPeriod(requestId: requestId, period: signer.period)
        } catch {
            diagnostics.logger.error("Chat request \(requestId) renewal into \(signer.period) failed: \(error)")
            diagnostics.issues.renewalPublishFailed.recordFailure(
                for: "\(requestId)-\(signer.period)",
                error: error,
                counters: [:]
            )
        }
    }

    func resendMissing(_ current: [ChatRequestRenewalCandidate]) async {
        for candidate in current {
            do {
                let signer = try signers.anonymous(requestId: candidate.message.messageId, period: candidate.period)
                let isStored = try await outgoingService.isStored(
                    to: candidate.session.peer,
                    ownKeyId: candidate.session.own,
                    signedBy: signer.signer.accountId
                )

                guard !isStored else { continue }

                try await publish(candidate, from: signer)
            } catch {
                diagnostics.logger.error("Chat request \(candidate.message.messageId) re-send failed: \(error)")
            }
        }
    }

    // A copy is evicted once its period and the grace window pass, so a request skipped beyond that is lost.
    func reportIfStarved(_ unclaimed: [ChatRequestRenewalCandidate], period: UInt32) {
        let starved = unclaimed.filter { $0.period + 1 < period }.count

        guard starved > 0 else { return }

        diagnostics.issues.renewalStarved.recordFailure(
            for: "\(period)",
            error: nil,
            counters: ["skipped": starved, "period": Int(period)]
        )
    }

    func publish(_ candidate: ChatRequestRenewalCandidate, from signer: ChatRequestDeliverySigner) async throws {
        try await outgoingService.send(
            message: candidate.message,
            to: candidate.session.peer,
            ownKeyId: candidate.session.own,
            signer: signer.signer
        )
    }
}
