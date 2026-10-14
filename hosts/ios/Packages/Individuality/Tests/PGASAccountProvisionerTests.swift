import BigInt
import Clocks
import IssueMonitoring
import Foundation
import SubstrateSdk
import Testing
@testable import Individuality

struct PGASAccountProvisionerTests {
    private static let account = Data(repeating: 0x0A, count: 32)
    private static let required: BigUInt = 500_000_000
    private static let claimAmount: BigUInt = 10_000_000_000

    private static let claimTimeout: Duration = .seconds(120)

    private let clock = TestClock<Duration>()
    private let allowance: FakeAllowanceManager
    private let balances = FakeBalanceProvider()
    private let provisioner: PGASAccountProvisioner

    init() {
        allowance = FakeAllowanceManager(clock: clock)
        allowance.balances = balances
        provisioner = PGASAccountProvisioner(
            allowanceManager: allowance,
            balanceProvider: balances,
            issueDiagnostics: .make(flow: "pgas-allowance", reporter: NoopIssueReporter()),
            claimTimeout: Self.claimTimeout,
            clock: clock
        )
    }

    @Test("an empty account is funded with a claim that keeps any existing allocation")
    func fundsEmptyAccount() async throws {
        allowance.topUp = Self.claimAmount

        try await provisioner.ensureFunded(account: Self.account)

        #expect(allowance.claims.map(\.policy) == [.ignore])
        #expect(allowance.claims.map(\.account) == [Self.account])
    }

    @Test("a funded account claims nothing")
    func fundedAccountClaimsNothing() async throws {
        balances.balance = Self.claimAmount

        try await provisioner.ensureFunded(account: Self.account)

        #expect(allowance.claims.isEmpty)
    }

    @Test("a balance covering the requirement claims nothing")
    func coveredClaimsNothing() async throws {
        balances.balance = Self.required

        try await provisioner.ensureCovers(account: Self.account, required: Self.required)

        #expect(allowance.claims.isEmpty)
    }

    @Test("a short balance is topped up exactly once")
    func shortBalanceToppedUp() async throws {
        balances.balance = Self.required - 1
        allowance.topUp = Self.claimAmount

        try await provisioner.ensureCovers(account: Self.account, required: Self.required)

        #expect(allowance.claims.map(\.policy) == [.increase])
    }

    @Test("a top-up that still leaves the balance short fails the attempt")
    func stillShortFails() async throws {
        balances.balance = 1
        allowance.topUp = 1

        await #expect(throws: PGASShortError(required: Self.required, available: 2)) {
            try await provisioner.ensureCovers(account: Self.account, required: Self.required)
        }
    }

    @Test("a claim that cannot be established fails the attempt")
    func claimFails() async throws {
        allowance.error = FakeAllowanceError.notInRing

        await #expect(throws: FakeAllowanceError.notInRing) {
            try await provisioner.ensureCovers(account: Self.account, required: Self.required)
        }
    }

    @Test("a claim that never reports back fails the attempt instead of holding it forever")
    func claimTimesOut() async throws {
        allowance.neverReportsBack = true

        let attempt = Task { [provisioner] in
            try await provisioner.ensureCovers(account: Self.account, required: Self.required)
        }
        await settle()
        await clock.advance(by: Self.claimTimeout)

        await #expect(throws: PGASClaimTimeoutError.self) { try await attempt.value }
    }

    private func settle() async {
        for _ in 0 ..< 20 {
            await Task.yield()
        }
    }
}

// MARK: - Fakes

private enum FakeAllowanceError: Error, Equatable {
    case notInRing
}

private final class FakeBalanceProvider: PGASBalanceProviding, @unchecked Sendable {
    var balance: BigUInt = .zero

    func transferableBalance(of _: AccountId) async throws -> BigUInt {
        balance
    }
}

private final class FakeAllowanceManager: AllowanceManaging, @unchecked Sendable {
    private let clock: any Clock<Duration>
    var balances: FakeBalanceProvider?
    var topUp: BigUInt = .zero
    var error: Error?
    var neverReportsBack = false
    private(set) var claims: [(account: AccountId, policy: OnExistingAllowancePolicy)] = []

    init(clock: any Clock<Duration>) {
        self.clock = clock
    }

    func allocate(
        accountId: AccountId,
        policy: OnExistingAllowancePolicy,
        priority _: AllowanceRecord.Priority
    ) async throws {
        claims.append((accountId, policy))
        if let error { throw error }
        if neverReportsBack {
            // Sleeps on the test clock, so only an advance past the timeout can end it.
            try await clock.sleep(for: .seconds(3_600))
        }
        balances?.balance += topUp
    }
}
