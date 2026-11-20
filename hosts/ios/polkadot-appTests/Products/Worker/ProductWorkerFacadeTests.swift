import Foundation
import Keystore_iOS
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A product built for TrUAPI cannot be served by the native container: its
/// client reads the container's frames as `unsupported frame with discriminant
/// (0, 0)` and the container drops the reply as a `Transport error`, so the
/// product hears no answer to anything it asks.
struct ProductWorkerFacadeTests {
    @Test
    func anOperationLeftByTheNativeRuntimeStartsNoNativeWorkerOnTrUAPI() async throws {
        // Given
        let storage = UserDataStorageTestFacade()
        try await CoreDataProductOperationStore(storageFacade: storage).save(
            ProductOperationRecord(productId: try ProductId("dim2.testnet"), id: 7, label: nil, startedAt: .distantPast)
        )
        let nativeWorkers = RecordingNativeWorkerFactory()
        let facade = ProductWorkerFacade(storageFacade: storage, settingsManager: trUAPISettings()) { _ in nativeWorkers }

        // When
        facade.setup()

        // Then
        #expect(await nativeWorkers.firstStart(within: .seconds(2)) == nil)
    }
}

private final class RecordingNativeWorkerFactory: ProductWorkerFactory, @unchecked Sendable {
    private let (starts, continuation) = AsyncStream<ProductId>.makeStream(bufferingPolicy: .unbounded)

    func startWorker(productId: ProductId) async throws -> ProductWorkerRunning {
        continuation.yield(productId)
        throw Unavailable()
    }

    func firstStart(within limit: Duration) async -> ProductId? {
        await withTaskGroup(of: ProductId?.self) { group in
            group.addTask { [starts] in await starts.first { _ in true } }
            group.addTask {
                try? await Task.sleep(for: limit)
                return nil
            }
            let first = await group.next() ?? nil
            group.cancelAll()
            return first
        }
    }
}

private func trUAPISettings() -> SettingsManagerProtocol {
    let settings = InMemorySettingsManager()
    settings.set(value: true, for: .truApiRuntimeEnabled)
    return settings
}

private struct Unavailable: Error {}
