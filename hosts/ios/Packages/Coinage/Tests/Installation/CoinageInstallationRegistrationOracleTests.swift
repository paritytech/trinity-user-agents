import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import Revive
import Testing
@testable import Coinage

struct CoinageInstallationRegistrationOracleTests {
    private static let finalizedHash = Data(repeating: 0xF1, count: 32)
    private static let bestHash = Data(repeating: 0xB1, count: 32)

    private let repository = StubDataStoreRepository()
    private let oracle: MonotoneEffectOracle

    init() {
        oracle = CoinageInstallationRegistrationOracle.make(
            chainId: "asset-hub",
            dataStoreRepository: repository,
            logger: nil
        )
    }

    @Test("the domain lives on asset hub")
    func chain() {
        #expect(oracle.chainId == "asset-hub")
    }

    @Test("a registration whose installation is listed took effect")
    func listedTookEffect() async throws {
        listed(TestContracts.contract, at: Self.finalizedHash, [.test])
        listed(TestContracts.contract, at: Self.bestHash, [.test])

        let transaction = registration(of: .test)
        let scope = try await openPass(transaction)

        #expect(scope.provenCompleted(transaction, at: .finalized))
    }

    @Test("a registration listed only at the best head is not yet final")
    func listedAtBestOnly() async throws {
        listed(TestContracts.contract, at: Self.finalizedHash, [])
        listed(TestContracts.contract, at: Self.bestHash, [.test])

        let transaction = registration(of: .test)
        let scope = try await openPass(transaction)

        #expect(scope.provenCompleted(transaction, at: .best))
        #expect(!scope.provenCompleted(transaction, at: .finalized))
        #expect(scope.provenNotCompleted(transaction, at: .finalized))
    }

    @Test("another installation being listed says nothing about this one")
    func otherInstallationListed() async throws {
        listed(TestContracts.contract, at: Self.finalizedHash, [.other])
        listed(TestContracts.contract, at: Self.bestHash, [.other])

        let transaction = registration(of: .test)
        let scope = try await openPass(transaction)

        #expect(!scope.provenCompleted(transaction, at: .best))
        #expect(scope.provenNotCompleted(transaction, at: .best))
    }

    @Test("each registration is judged on the contract its group names")
    func perContract() async throws {
        listed(TestContracts.contract, at: Self.finalizedHash, [.test])
        listed(TestContracts.contract, at: Self.bestHash, [.test])
        listed(TestContracts.otherContract, at: Self.finalizedHash, [])
        listed(TestContracts.otherContract, at: Self.bestHash, [])

        let onFirst = registration(of: .test, contract: TestContracts.contract)
        let onSecond = registration(of: .test, contract: TestContracts.otherContract)
        let scope = try await openPass(onFirst, onSecond)

        #expect(scope.provenCompleted(onFirst, at: .finalized))
        #expect(scope.provenNotCompleted(onSecond, at: .finalized))
    }

    @Test("a failed read decides nothing")
    func failedRead() async throws {
        repository.failing(at: Self.finalizedHash)
        repository.failing(at: Self.bestHash)

        let transaction = registration(of: .test)
        let scope = try await openPass(transaction)

        for head in [HeadKind.finalized, .best] {
            #expect(!scope.provenCompleted(transaction, at: head))
            #expect(!scope.provenNotCompleted(transaction, at: head))
        }
    }

    @Test("a transaction whose group names no target decides nothing")
    func noTarget() async throws {
        listed(TestContracts.contract, at: Self.finalizedHash, [.test])
        listed(TestContracts.contract, at: Self.bestHash, [.test])

        for groupId in ["not-a-target", CoinageInstallationId.test.pageSegment] {
            let transaction = DurableTxEntry.fixture(domainId: .coinageInstallation, groupId: groupId)
            let scope = try await openPass(transaction)

            #expect(!scope.provenCompleted(transaction, at: .finalized))
            #expect(!scope.provenNotCompleted(transaction, at: .finalized))
        }
    }

    @Test("one read per head per contract however many registrations are live")
    func oneReadPerHead() async throws {
        listed(TestContracts.contract, at: Self.finalizedHash, [.test])
        listed(TestContracts.contract, at: Self.bestHash, [.test])

        let transactions = (0 ..< 25).map { _ in registration(of: .test) }
        _ = try await openPass(transactions)

        #expect(repository.reads == 2)
    }
}

private extension CoinageInstallationRegistrationOracleTests {
    func listed(_ contract: EvmAddress, at blockHash: Data, _ installations: Set<CoinageInstallationId>) {
        repository.listed(installations, contract: contract, at: blockHash)
    }

    func registration(
        of installation: CoinageInstallationId,
        contract: EvmAddress = TestContracts.contract
    ) -> DurableTxEntry {
        .registration(InstallationRegistrationTarget(contract: contract, installation: installation), status: .pending)
    }

    func openPass(_ transactions: DurableTxEntry...) async throws -> any TxCompletionPassScope {
        try await openPass(transactions)
    }

    func openPass(_ transactions: [DurableTxEntry]) async throws -> any TxCompletionPassScope {
        try await oracle.openPass(
            transactions: transactions,
            ledger: SnapshotLedgerView(transactions: transactions),
            view: StubPinnedChainView(
                chainId: "asset-hub",
                finalized: BlockRef(number: 130, hash: Self.finalizedHash),
                best: BlockRef(number: 140, hash: Self.bestHash)
            )
        )
    }
}
