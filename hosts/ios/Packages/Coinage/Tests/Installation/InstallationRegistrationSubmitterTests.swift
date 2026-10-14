import BigInt
import DurableTransactions
import Foundation
import Individuality
import Revive
import Testing
@testable import Coinage

struct InstallationRegistrationSubmitterTests {
    private static let deposit: BigUInt = 413_000_000
    private static let fee: BigUInt = 30_000_000
    private static let target = InstallationRegistrationTarget(contract: TestContracts.contract, installation: .test)

    private let dataStore = StubDataStoreRepository()
    private let revive = StubReviveApi()
    private let pgas = StubPgasProvisioner()
    private let feeEstimator = StubFeeEstimator()
    private let engine = RecordingEngine()
    private let origins = StubOriginFactory()
    private let submitter: InstallationRegistrationSubmitter

    init() {
        submitter = InstallationRegistrationSubmitter(
            dataStoreRepository: dataStore,
            reviveApi: revive,
            pgasProvisioner: pgas,
            feeEstimator: feeEstimator,
            callArguments: StubReviveCallArguments(),
            chainId: "asset-hub",
            originFactory: origins,
            engine: engine,
            logger: nil
        )
    }

    @Test("an attempt is funded, checked, simulated and handed to the engine under its target's group")
    func happyPath() async throws {
        let id = try await submitter.submitAttempt(target: Self.target)

        #expect(id == engine.submittedId)
        #expect(pgas.funded == [dataStore.account.accountId])
        #expect(revive.mappingChecks == [dataStore.account.accountId])
        #expect(revive.dryRuns.count == 1)
        #expect(revive.dryRuns.first?.origin == dataStore.account.accountId)
        #expect(revive.dryRuns.first?.contract == TestContracts.contract)
        #expect(pgas.covered.count == 1)
        #expect(engine.submissions.count == 1)
        #expect(engine.submissions.first?.domain == .coinageInstallation)
        #expect(engine.submissions.first?.groupId == Self.target.registrationGroup)
        #expect(feeEstimator.estimates == 1)
        #expect(origins.signedOriginChainIds == ["asset-hub", "asset-hub"])
    }

    @Test("the balance required is the storage deposit with its margin plus the fee")
    func requiredBalance() async throws {
        _ = try await submitter.submitAttempt(target: Self.target)

        #expect(pgas.covered.first?.required == Self.deposit * 120 / 100 + Self.fee)
    }

    @Test("an unmapped data store account fails the attempt before anything is signed")
    func unmapped() async throws {
        revive.mapped = false

        await #expect(throws: DataStoreAccountUnmappedError.self) {
            try await submitter.submitAttempt(target: Self.target)
        }
        #expect(revive.dryRuns.isEmpty)
        #expect(engine.submissions.isEmpty)
    }

    @Test("an account that could not be funded is never simulated")
    func fundingFailed() async throws {
        pgas.fundingError = InstallationStubError.notEnoughPgas

        await #expect(throws: InstallationStubError.notEnoughPgas) {
            try await submitter.submitAttempt(target: Self.target)
        }
        #expect(revive.dryRuns.isEmpty)
    }

    @Test("a call the contract would revert is never submitted")
    func reverted() async throws {
        revive.dryRunResult = .failure(ReviveContractRevertedError(data: Data([0x04])))

        await #expect(throws: ReviveContractRevertedError.self) {
            try await submitter.submitAttempt(target: Self.target)
        }
        #expect(engine.submissions.isEmpty)
    }

    @Test("a balance that cannot cover the call is never submitted")
    func short() async throws {
        pgas.coverError = PGASShortError(required: Self.fee, available: 1)

        await #expect(throws: PGASShortError.self) {
            try await submitter.submitAttempt(target: Self.target)
        }
        #expect(engine.submissions.isEmpty)
    }
}
