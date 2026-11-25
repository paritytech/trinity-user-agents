import BackgroundExecution
import Foundation
import Individuality
import MessageExchangeKit
import SDKLogger
import StatementDelivery
import StatementStore
import StructuredConcurrency
import SubstrateSdk

protocol ChatRequestCancelServicing {
    func cancel(requestId: String) async throws
}

enum ChatRequestCancelError: Error {
    case notPending
}

protocol PendingOutgoingChatRequestLoading {
    func pendingRequest(requestId: String) async throws -> OutgoingChatRequestInput?
}

struct ChatRequestCancelDependencies {
    let loader: PendingOutgoingChatRequestLoading
    let delivery: any OutgoingStatementDelivering<OutgoingChatRequestInput>
    let builder: any ChatRequestStatementBuilding
    let signerManager: StatementStoreSignerManaging
    let periodProvider: NotificationParametersProviding
    let connection: StatementStoreConnecting
    let priorityFactory: StatementPriorityMaking
    let removalCleanup: ChatRemovalCleanupServicing
    let requestStore: ChatRequestStoreServicing
    let backgroundExecutor: BackgroundExecuting
    let logger: SDKLoggerProtocol
    let clock: any Clock<Duration>
}

private struct OwnRequestCopy {
    let statement: Statement
    let signer: StatementStoreSigning
}

/// Waits double from 1 s, so 10 attempts span about 8.5 minutes; iOS ending background time cancels earlier.
private enum CancelRetry {
    static let maxAttempts = 10
    static let firstDelay: Duration = .seconds(1)
}

/// Marks the request cancelled and removes its contact and chat at once, then publishes one cancel per live copy,
/// each signed by the copy's own account. Connection failures are retried with backoff while the task runs; a cancel
/// that still fails is only logged and the store evicts the copy later.
final class ChatRequestCancelService {
    private let dependencies: ChatRequestCancelDependencies
    private let submitter: SigningStatementSubmitter

    init(dependencies: ChatRequestCancelDependencies) {
        self.dependencies = dependencies
        submitter = SigningStatementSubmitter(connection: dependencies.connection)
    }
}

extension ChatRequestCancelService: ChatRequestCancelServicing {
    func cancel(requestId: String) async throws {
        guard let pending = try await dependencies.loader.pendingRequest(requestId: requestId) else {
            throw ChatRequestCancelError.notPending
        }

        try await dependencies.requestStore.markOutgoingRequestCancelled(requestId)
        await cleanUpAfterRemoval(of: pending)

        try await dependencies.backgroundExecutor.execute { [self] in
            await publishCancel(for: pending)
        }
    }
}

private extension ChatRequestCancelService {
    func cleanUpAfterRemoval(of pending: OutgoingChatRequestInput) async {
        do {
            try await dependencies.removalCleanup.cleanUpAfterRemoval(of: pending.session.peer.accountId)
        } catch {
            dependencies.logger.error("Cleanup after request cancel failed: \(error)")
        }
    }

    // Copies are read after retire: a first publish that finished during the wait must still be cancelled.
    func publishCancel(for pending: OutgoingChatRequestInput) async {
        await dependencies.delivery.retire(pending.statementId)

        do {
            let input = try makeCancelInput(from: pending)

            let copies = try await retryingTransientFailures {
                try await self.fetchOwnCopies(for: input)
            }

            for copy in copies {
                await submitCancel(input, replacing: copy)
            }
        } catch {
            dependencies.logger.warning("Request cancel was not published: \(error)")
        }
    }

    func retryingTransientFailures<T: Sendable>(
        _ operation: @Sendable @escaping () async throws -> T
    ) async throws -> T {
        try await withRetry(
            maxAttempts: CancelRetry.maxAttempts,
            initialDelay: CancelRetry.firstDelay,
            clock: dependencies.clock,
            shouldRetry: { $0.allowsCancelRetry },
            operation: operation
        )
    }

    func submitCancel(_ input: OutgoingChatRequestInput, replacing copy: OwnRequestCopy) async {
        do {
            try await retryingTransientFailures {
                let unsigned = try self.dependencies.builder.makeUnsignedStatement(
                    for: input,
                    expiry: self.expiry(after: copy.statement)
                )
                try await self.submitter.submit(unsigned, signer: copy.signer)
            }
        } catch {
            dependencies.logger.warning("Request cancel failed for one copy: \(error)")
        }
    }

    func fetchOwnCopies(for input: OutgoingChatRequestInput) async throws -> [OwnRequestCopy] {
        let stored = try await fetchStoredStatements(for: input)
        let period = try await dependencies.periodProvider.currentPeriod()
        let signers = try candidateKeyIds(for: input, period: period).map {
            try dependencies.signerManager.makeSigner(for: $0)
        }

        return stored.compactMap { statement in
            guard
                let sender = statement.getSenderAccountId(),
                let signer = signers.first(where: { $0.accountId == sender })
            else {
                dependencies.logger.warning("Skipping a request copy from an unknown account")
                return nil
            }

            return OwnRequestCopy(statement: statement, signer: signer)
        }
    }

    func candidateKeyIds(for input: OutgoingChatRequestInput, period: UInt32) throws -> [String] {
        let currentOneShotKeyId = try input.oneShotKeyId(period: period)
        var keyIds = [input.identitySignKeyId, currentOneShotKeyId]

        if period > 0 {
            try keyIds.append(input.oneShotKeyId(period: period - 1))
        }

        return keyIds
    }

    func makeCancelInput(from request: OutgoingChatRequestInput) throws -> OutgoingChatRequestInput {
        let cancelProof = try dependencies.requestStore.makeOutgoingCancelProof(
            requestId: request.message.messageId,
            peerEncryptionPubKey: request.session.peer.publicKey,
            ownEncryptionKeyId: request.session.own.encryptionKeyId
        )

        let cancelMessage = Chat.RequestMessage(
            messageId: request.message.messageId,
            timestamp: Date().toChatTimestamp(),
            content: .cancel(Chat.RequestContentCancel(identityProof: cancelProof))
        )

        return OutgoingChatRequestInput(message: cancelMessage, session: request.session)
    }

    func fetchStoredStatements(for input: OutgoingChatRequestInput) async throws -> [Statement] {
        let filter = try dependencies.builder.storageTopicFilter(for: input)
        let encoded = try await dependencies.connection.fetchStatements(with: filter)

        return encoded.compactMap { try? Statement(scaleDecoder: ScaleDecoder(data: $0)) }
    }

    // Expiry must exceed the copy's own previous statement on the channel, or the store keeps the old one.
    func expiry(after statement: Statement) -> UInt64 {
        let timestampPriority = dependencies.priorityFactory.makeTimestampPriority()

        guard let copyExpiry = statement.getExpiry() else {
            return timestampPriority
        }

        let (next, overflow) = copyExpiry.addingReportingOverflow(1)
        return overflow ? .max : max(next, timestampPriority)
    }
}

private extension Error {
    // A store rejection is final, except contention on the channel; anything else is a connection problem.
    var allowsCancelRetry: Bool {
        guard let submitError = self as? StatementSubmitError else { return true }

        return submitError == .rejected(.channelPriorityTooLow)
    }
}
