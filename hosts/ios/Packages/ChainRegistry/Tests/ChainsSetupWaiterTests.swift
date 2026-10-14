import Testing
import Foundation
import os
import Operation_iOS
import SubstrateSdk
import ChainRegistry

struct ChainsSetupWaiterTests {
    @Test func cancellationUnsubscribesExactlyOnce() async throws {
        let double = MockChainRegistry()
        let chainIds: Set<ChainModel.Id> = ["polkadot", "kusama"]

        let waitTask = Task { await double.asyncWaitChainsSetup(for: chainIds) }

        let subscribed = try await poll { double.hasSubscription }
        #expect(subscribed, "Subscription should be established before cancellation")

        waitTask.cancel()

        let unsubscribed = try await poll { double.unsubscribeCount > 0 }
        #expect(unsubscribed, "Unsubscribe should complete before timeout")
        #expect(double.unsubscribeCount == 1, "Unsubscribe should be called exactly once on cancellation")
    }

    @Test func normalPathUnsubscribesExactlyOnce() async throws {
        let double = MockChainRegistry()
        let chainIds: Set<ChainModel.Id> = ["polkadot", "kusama"]

        Task { await double.asyncWaitChainsSetup(for: chainIds) }

        let subscribed = try await poll { double.hasSubscription }
        #expect(subscribed, "Subscription should be established")

        double.deliverChainInsert(makeChain(id: "polkadot"))
        double.deliverChainInsert(makeChain(id: "kusama"))

        let unsubscribed = try await poll { double.unsubscribeCount > 0 }
        #expect(unsubscribed, "Unsubscribe should complete before timeout")
        #expect(double.unsubscribeCount == 1, "Unsubscribe should be called exactly once on success")
    }
}

// MARK: - Helpers

/// Waits for `condition` to become true, returning whether it did before the attempts ran out.
/// Total wait time is approximately 3 seconds (300 iterations * 10ms).
private func poll(until condition: () -> Bool) async throws -> Bool {
    for _ in 0 ..< 300 {
        if condition() {
            return true
        }

        try await Task.sleep(for: .milliseconds(10))
    }

    return false
}

private func makeChain(id: ChainModel.Id) -> ChainModel {
    ChainModel(
        chainId: id,
        parentId: nil,
        name: id,
        assets: [],
        nodes: [],
        nodeSwitchStrategy: .roundRobin,
        addressPrefix: 0,
        explicitGenesisHash: nil,
        types: nil,
        icon: nil,
        options: nil,
        externalApis: nil,
        explorers: nil,
        order: 0,
        additional: nil,
        syncMode: .full
    )
}
