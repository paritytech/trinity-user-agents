import BigInt
import Foundation
import StructuredConcurrency
import SubstrateSdk

/// Reads the transferable PGAS balance of an account.
public protocol PGASBalanceProviding: Sendable {
    func transferableBalance(of accountId: AccountId) async throws -> BigUInt
}

/// Keeps an account funded with PGAS: enough to be simulated against, then enough to cover a call.
public protocol PGASAccountProvisioning: Sendable {
    /// An empty account cannot even be simulated against: the dry-run fails on the deposit it cannot
    /// reserve. Claims when the balance is zero, keeping any existing allocation.
    func ensureFunded(account: AccountId) async throws

    /// Tops the account up once when it holds less than `required`; fails with ``PGASShortError`` when
    /// it still does after the top-up.
    func ensureCovers(account: AccountId, required: BigUInt) async throws
}

public struct PGASShortError: Error, Equatable {
    public let required: BigUInt
    public let available: BigUInt

    public init(required: BigUInt, available: BigUInt) {
        self.required = required
        self.available = available
    }
}

public struct PGASClaimTimeoutError: Error, Equatable {
    public init() {}
}

/// The provisioned account pays both the fee and the storage deposit in PGAS. Claiming takes a ring
/// proof, which the allowance manager waits for — a fresh account simply waits here.
public final class PGASAccountProvisioner: PGASAccountProvisioning, @unchecked Sendable {
    private let allowanceManager: any AllowanceManaging
    private let balanceProvider: any PGASBalanceProviding
    private let issueDiagnostics: AllowanceIssueDiagnostics
    private let claimTimeout: Duration
    private let clock: any Clock<Duration>

    public init(
        allowanceManager: any AllowanceManaging,
        balanceProvider: any PGASBalanceProviding,
        issueDiagnostics: AllowanceIssueDiagnostics,
        claimTimeout: Duration = .seconds(2 * 60),
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        self.allowanceManager = allowanceManager
        self.balanceProvider = balanceProvider
        self.issueDiagnostics = issueDiagnostics
        self.claimTimeout = claimTimeout
        self.clock = clock
    }

    public func ensureFunded(account: AccountId) async throws {
        let transferable = try await balanceProvider.transferableBalance(of: account)
        guard transferable == .zero else { return }

        try await claim(account: account, policy: .ignore)
    }

    public func ensureCovers(account: AccountId, required: BigUInt) async throws {
        let transferable = try await balanceProvider.transferableBalance(of: account)
        guard transferable < required else { return }

        try await claim(account: account, policy: .increase)

        let topped = try await balanceProvider.transferableBalance(of: account)
        guard topped >= required else {
            throw PGASShortError(required: required, available: topped)
        }
    }
}

private extension PGASAccountProvisioner {
    /// The claim waits for ring inclusion with no bound of its own, and a status stream that goes quiet
    /// would hold the caller forever. Giving up is safe: a retry reads the balance first, and a claim
    /// for the same slot proves the same alias, which the chain accepts once.
    func claim(account: AccountId, policy: OnExistingAllowancePolicy) async throws {
        try await issueDiagnostics.observeAllocation {
            do {
                try await withTimeout(claimTimeout, clock: clock) { [allowanceManager] in
                    try await allowanceManager.allocate(accountId: account, policy: policy, priority: .normal)
                }
            } catch is TimeoutError {
                throw PGASClaimTimeoutError()
            }
        }
    }
}
