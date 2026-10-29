import Foundation
import FoundationExt
import Individuality
import MessageExchangeKit
import os
import StatementStore
import SubstrateSdk

/// Signs chat statements with the per-chat account, funding it first and retrying with backoff until funded.
/// When no slot can be allocated this period, it falls back to the identity account with the user's consent.
final class ChatAccountFunding: @unchecked Sendable {
    private struct State {
        var consecutiveFailures: [AccountId: Int] = [:]
        var inFlight: [AccountId: Task<Void, Error>] = [:]
        // Periods in which no slot could be allocated; the fallback signs until the next period.
        var exhaustedPeriods: [AccountId: UInt32] = [:]
    }

    private static let firstRetryDelay: Duration = .seconds(2)
    private static let maxRetryDelay: Duration = .seconds(5 * 60)
    private static let maxBackoffDoublings = 8

    private let funding: StatementAccountFunding
    private let signerManager: ChatPeerStatementSignerManaging
    private let identityFallback: NonPrivateSigningFallbacking
    private let dateProvider: DateProviding
    private let clock: any Clock<Duration>
    private let issueDiagnostics: ChatAccountFundingIssueDiagnostics
    private let state = OSAllocatedUnfairLock(initialState: State())

    init(
        funding: StatementAccountFunding,
        signerManager: ChatPeerStatementSignerManaging,
        identityFallback: NonPrivateSigningFallbacking,
        dateProvider: DateProviding,
        issueDiagnostics: ChatAccountFundingIssueDiagnostics,
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        self.funding = funding
        self.signerManager = signerManager
        self.identityFallback = identityFallback
        self.dateProvider = dateProvider
        self.issueDiagnostics = issueDiagnostics
        self.clock = clock
    }
}

extension ChatAccountFunding: PeerSessionSignerResolving {
    func signer(for request: MessageExchange.SessionRequest) async throws -> StatementStoreSigning {
        let signer = try signerManager.makeSigner(with: request.peer.accountId)

        do {
            try await awaitFunded(accountId: signer.accountId)
            return signer
        } catch StatementStoreAllowanceError.noEvictableSlots {
            return try await identityFallback.identitySigner(
                for: request.peer.accountId,
                ownSignKeyId: request.own.signKeyId
            )
        }
    }
}

extension ChatAccountFunding {
    func awaitFunded(accountId: AccountId) async throws {
        while try await !isFundedForCurrentPeriod(accountId) {
            let period = await currentPeriod()

            guard state.withLock({ $0.exhaustedPeriods[accountId] != period }) else {
                throw StatementStoreAllowanceError.noEvictableSlots
            }

            do {
                try await fundingTask(for: accountId).value
                return
            } catch StatementStoreAllowanceError.noEvictableSlots {
                throw StatementStoreAllowanceError.noEvictableSlots
            } catch {
                try await clock.sleep(for: retryDelay(for: accountId))
            }
        }
    }
}

private extension ChatAccountFunding {
    // Same period arithmetic as the allocator: one-day periods on the device clock.
    func currentPeriod() async -> UInt32 {
        await UInt32(dateProvider.read().timeIntervalSince1970 / .secondsInDay)
    }

    func isFundedForCurrentPeriod(_ accountId: AccountId) async throws -> Bool {
        try await funding.hasAllocation(accountId: accountId, coveringPeriod: currentPeriod())
    }

    func fundingTask(for accountId: AccountId) -> Task<Void, Error> {
        state.withLock { state in
            if let existing = state.inFlight[accountId] {
                return existing
            }

            let task = Task { try await fund(accountId) }
            state.inFlight[accountId] = task
            return task
        }
    }

    func fund(_ accountId: AccountId) async throws {
        do {
            // A caller may have read the record just before a previous funding finished.
            if try await !isFundedForCurrentPeriod(accountId) {
                try await funding.ensureFunded(accountId: accountId, priority: .critical, domain: .chat)
            }

            state.withLock { state in
                state.consecutiveFailures[accountId] = nil
                state.inFlight[accountId] = nil
            }

            issueDiagnostics.fundingFailing.recordRecovery(for: accountId.toHex())
        } catch StatementStoreAllowanceError.noEvictableSlots {
            let period = await currentPeriod()

            state.withLock { state in
                state.exhaustedPeriods[accountId] = period
                state.inFlight[accountId] = nil
            }

            throw StatementStoreAllowanceError.noEvictableSlots
        } catch {
            state.withLock { state in
                state.consecutiveFailures[accountId, default: 0] += 1
                state.inFlight[accountId] = nil
            }

            issueDiagnostics.fundingFailing.recordFailure(for: accountId.toHex(), error: error, counters: [:])
            throw error
        }
    }

    func retryDelay(for accountId: AccountId) -> Duration {
        let failures = state.withLock { $0.consecutiveFailures[accountId] ?? 1 }
        let doublings = min(max(failures - 1, 0), Self.maxBackoffDoublings)
        return min(Self.firstRetryDelay * (1 << doublings), Self.maxRetryDelay)
    }
}
