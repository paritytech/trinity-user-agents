import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import SubstrateSdk
import Testing
@testable import Coinage

/// The reservation guard. A coin whose keys have already left — or are about to — must not be
/// reservable for a second send: both memos would carry the same key and only one peer could claim.
///
/// The check belongs inside the ledger's write transaction, which is where `preCommitHandoff` runs its
/// validation, so nothing can move between the read and the mark.
@Suite("Coinage handoff reservation")
struct CoinageTxServiceHandoffTests {
    @Test("a coin already reserved cannot be reserved again")
    func doubleReservationIsRejected() async throws {
        let service = try makeService()
        let coin = try makeCoin(item: 1)

        _ = try await service.preCommitHandoff([coin])

        await #expect(throws: CoinageTxError.self) {
            _ = try await service.preCommitHandoff([coin])
        }
    }

    @Test("a coin whose handoff was committed cannot be reserved again")
    func reservationAfterCommitIsRejected() async throws {
        let repository = MockCoinageTxRepository()
        let service = try makeService(repository: repository)
        let coin = try makeCoin(item: 2)

        let handle = try await service.preCommitHandoff([coin])
        try handle.commit(in: InMemoryRegistrationScope())

        await #expect(throws: CoinageTxError.self) {
            _ = try await service.preCommitHandoff([coin])
        }
    }

    @Test("an unrelated coin is unaffected by another's reservation")
    func unrelatedCoinStillReservable() async throws {
        let service = try makeService()
        let reserved = try makeCoin(item: 3)
        let other = try makeCoin(item: 4)

        _ = try await service.preCommitHandoff([reserved])

        // Must not throw: the guard is per asset, not a blanket refusal once anything is handed off.
        _ = try await service.preCommitHandoff([other])
    }
}

private extension CoinageTxServiceHandoffTests {
    func makeService(repository: MockCoinageTxRepository = MockCoinageTxRepository()) throws -> CoinageTxService {
        CoinageTxService(
            engine: FakeRegistrationEngine(),
            ledger: repository.ledger,
            logger: nil
        )
    }

    func makeCoin(item: DerivationIndex) throws -> OwnAsset {
        try .coin(
            CoinageKeyIndex(installation: .test, item: item),
            Data.randomOrError(of: 32)
        )
    }
}
