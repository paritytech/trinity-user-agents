import Testing
import Foundation
import SubstrateSdk
import NovaCrypto
import Operation_iOS
@testable import Coinage

struct VoucherAllocatorTests {
    private let store = StubCurrentInstallationStore(current: .test)
    private let mockDelay = MockDelayProvider()
    private let allocator: VoucherAllocator

    init() {
        allocator = VoucherAllocator(
            installationStore: store,
            delayProvider: mockDelay,
            voucherRepository: AnyDataProviderRepository(StubRepository<Voucher>()),
            keyFactory: VoucherKeypairFactory(entropyManager: MockEntropyManager(entropy: Data(
                repeating: 0x01,
                count: 32
            )))
        )
    }

    @Test("an allocated voucher takes the next item of the current installation")
    func allocateVoucher() async throws {
        store.voucherItem = 7
        mockDelay.interval = 3_600

        let startTime = Date()
        let voucher = try await allocator.allocate(exponent: -2)
        let endTime = Date()

        #expect(voucher.derivationIndex == CoinageKeyIndex(installation: .test, item: 7))
        #expect(store.voucherRequests == 1)
        #expect(store.coinRequests == 0)
        #expect(voucher.exponent == -2)
        #expect(voucher.recycler == nil)
        #expect(voucher.allocatedAt >= startTime)
        #expect(voucher.allocatedAt <= endTime)
        #expect(voucher.readyAt == voucher.allocatedAt.addingTimeInterval(3_600))
    }

    @Test("an installation with no vouchers yet starts at item zero and never repeats an item")
    func consecutiveItems() async throws {
        let first = try await allocator.allocate(exponent: 0)
        let second = try await allocator.allocate(exponent: 0)

        #expect(first.derivationIndex == CoinageKeyIndex(installation: .test, item: 0))
        #expect(second.derivationIndex == CoinageKeyIndex(installation: .test, item: 1))
    }

    @Test("propagates errors from the installation store")
    func allocationFailures() async throws {
        store.error = InstallationStubError.unreachable

        await #expect(throws: InstallationStubError.unreachable) {
            try await allocator.allocate(exponent: 0)
        }
    }
}

// MARK: - Mocks

private extension VoucherAllocatorTests {
    final class MockDelayProvider: VoucherDelayProviderProtocol {
        var interval: TimeInterval = 0
        func timeInterval() -> TimeInterval {
            interval
        }
    }
}
