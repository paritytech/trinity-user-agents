import Foundation
import Keystore_iOS
import Testing
@testable import Coinage

struct CoinageCurrentInstallationStoreTests {
    private let keychain = InMemoryKeychain()
    private let repository = InMemoryCurrentInstallationRepository()
    private let tags = StubInstallationKeychainTags()
    private let store: CoinageCurrentInstallationStore

    init() {
        store = CoinageCurrentInstallationStore(repository: repository, keystore: keychain, tags: tags)
    }

    @Test("the current installation is created once, then read from memory")
    func createdOnce() async throws {
        let first = try await store.getOrCreateCurrent()
        let second = try await store.getOrCreateCurrent()
        _ = try await store.nextCoinItem()

        #expect(first == second)
        #expect(repository.current == first)
        #expect(repository.reads == 1)
    }

    @Test("concurrent first callers share one installation")
    func concurrentCreation() async throws {
        let ids = try await withThrowingTaskGroup(of: CoinageInstallationId.self) { group in
            for _ in 0 ..< 8 {
                group.addTask { try await store.getOrCreateCurrent() }
            }
            return try await group.reduce(into: Set<CoinageInstallationId>()) { $0.insert($1) }
        }

        #expect(ids.count == 1)
        #expect(repository.reads == 1)
    }

    @Test("a read that failed is not kept: the next caller reads again")
    func failedReadRetried() async throws {
        repository.error = InstallationStubError.unreachable

        await #expect(throws: InstallationStubError.unreachable) { try await store.getOrCreateCurrent() }

        repository.error = nil
        _ = try await store.getOrCreateCurrent()
        #expect(repository.reads == 2)
    }

    @Test("a new store over the same keychain and database sees the same installation and counters")
    func survivesNewInstance() async throws {
        let installation = try await store.getOrCreateCurrent()
        #expect(try await store.nextCoinItem() == 0)
        #expect(try await store.nextCoinItem() == 1)

        let reopened = CoinageCurrentInstallationStore(repository: repository, keystore: keychain, tags: tags)

        #expect(try await reopened.getOrCreateCurrent() == installation)
        #expect(try await reopened.nextCoinItem() == 2)
    }

    @Test("coin and voucher counters start at zero and advance independently")
    func independentCounters() async throws {
        #expect(try await store.nextCoinItem() == 0)
        #expect(try await store.nextCoinItem() == 1)
        #expect(try await store.nextVoucherItem() == 0)
        #expect(try await store.nextCoinItem() == 2)
        #expect(try await store.nextVoucherItem() == 1)
    }

    /// A same-device restore brings the Keychain back but not the database: the old installation's
    /// counters must stay where they are and a fresh installation must count from zero, so no key of
    /// the old subtree is reissued and none of the new one is skipped.
    @Test("a lost database starts a new installation with zeroed counters, leaving the old ones intact")
    func lostDatabase() async throws {
        let lost = try await store.getOrCreateCurrent()
        _ = try await store.nextCoinItem()
        _ = try await store.nextCoinItem()
        _ = try await store.nextVoucherItem()

        repository.dropRow()
        let restarted = CoinageCurrentInstallationStore(repository: repository, keystore: keychain, tags: tags)
        let fresh = try await restarted.getOrCreateCurrent()

        #expect(fresh != lost)
        #expect(try await restarted.nextCoinItem() == 0)
        #expect(try await restarted.nextVoucherItem() == 0)
        #expect(try keychain.fetchKey(for: tags.coinIndexTag(for: lost)) == DerivationIndex(2).scaleEncoded())
        #expect(try keychain.fetchKey(for: tags.voucherIndexTag(for: lost)) == DerivationIndex(1).scaleEncoded())
    }

    @Test("a counter with trailing bytes is reported as corrupted")
    func trailingBytes() async throws {
        let tag = try await tags.coinIndexTag(for: store.getOrCreateCurrent())
        try keychain.saveKey(Data([0x01, 0, 0, 0, 0, 0, 0, 0, 0xFF]), with: tag)

        await #expect(throws: CoinageCurrentInstallationStoreError.corruptedRecord(tag)) {
            try await store.nextCoinItem()
        }
    }

    @Test("an exhausted counter refuses to wrap around")
    func counterExhausted() async throws {
        let tag = try await tags.voucherIndexTag(for: store.getOrCreateCurrent())
        try keychain.saveKey(DerivationIndex.max.scaleEncoded(), with: tag)

        await #expect(throws: CoinageCurrentInstallationStoreError.counterExhausted(tag)) {
            try await store.nextVoucherItem()
        }
    }

    @Test("an undecodable counter is reported as corrupted")
    func corruptedCounter() async throws {
        let tag = try await tags.coinIndexTag(for: store.getOrCreateCurrent())
        try keychain.saveKey(Data([0x01]), with: tag)

        await #expect(throws: CoinageCurrentInstallationStoreError.corruptedRecord(tag)) {
            try await store.nextCoinItem()
        }
    }
}
