import Coinage
import DurableTransactions
import Foundation
import Testing

@testable import polkadot_app

extension CoreDataBenchmarks {
    @Suite("Recycling", .serialized)
    struct Recycling {
        let scale = BenchmarkScale.default

        /// Issue #156 in miniature: per-coin voucher saves, one batch registration, then one save per
        /// status transition, with unfiltered coin and voucher subscriptions live and a reader polling
        /// a chat the way an open screen would.
        @Test("B4 coin-recycling replica", .timeLimit(.minutes(5)), arguments: StackVariant.supported)
        func coinRecyclingReplica(variant: StackVariant) async throws {
            let facade = try BenchmarkStorageFacade(variant: variant)
            let seeder = BenchmarkSeeder(facade: facade)

            let chatIds = try await seeder.seedChats(
                count: scale.recyclingChats,
                messagesPerChat: scale.messagesPerChat
            )
            let coins = try await seeder.seedCoins(count: scale.recyclingCoins)

            let subscriptions = ProductionFloorSubscriptions(
                facade: facade,
                chatIds: chatIds,
                groups: [.messages, .coins, .vouchers]
            )
            subscriptions.start()
            await subscriptions.tracker.waitForSize(
                subscription: ProductionFloorSubscriptions.unfilteredCoins,
                atLeast: coins.count
            )

            let readers = LatencyRecorder(scenario: "B4.read", variant: variant)
            let voucherSaves = LatencyRecorder(scenario: "B4.voucherSave", variant: variant)
            let registration = LatencyRecorder(scenario: "B4.registration", variant: variant)
            let statusSaves = LatencyRecorder(scenario: "B4.statusSave", variant: variant)
            let wall = LatencyRecorder(scenario: "B4.wall", variant: variant)

            let readerTask = Task {
                await seeder.runPeriodicReader(chatIds: chatIds, interval: .milliseconds(20), recorder: readers)
            }

            let wallStart = ContinuousClock.now

            for index in 0 ..< coins.count {
                try await voucherSaves.time { try await seeder.saveVoucher(index: 10_000 + index) }
            }

            let ledger = CoinageCoreDataLedger(storageFacade: facade)
            let registrations = (0 ..< coins.count).map {
                BenchmarkFixtures.registration(index: $0, groupId: "bench-recycle")
            }
            let ids = try await registration.time { try await ledger.register(registrations) }

            for (id, registered) in zip(ids, registrations) {
                _ = try await statusSaves.time {
                    try await ledger.durable.updateTxStatus(
                        for: id,
                        expectedCurrentStatus: .pending,
                        expectedTxHash: registered.txHash,
                        verdict: Verdict(status: .pendingSuccess, successDetectedAt: nil)
                    )
                }
                _ = try await statusSaves.time {
                    try await ledger.durable.updateTxStatus(
                        for: id,
                        expectedCurrentStatus: .pendingSuccess,
                        expectedTxHash: registered.txHash,
                        verdict: Verdict(status: .finalizedSuccess, successDetectedAt: nil)
                    )
                }
            }

            wall.record(since: wallStart)

            readerTask.cancel()
            await readerTask.value
            subscriptions.stop()

            var counters = subscriptions.counters
            counters["coinDeliveries"] = subscriptions.tracker.deliveries(
                subscription: ProductionFloorSubscriptions.unfilteredCoins
            )
            counters["voucherDeliveries"] = subscriptions.tracker.deliveries(
                subscription: ProductionFloorSubscriptions.unfilteredVouchers
            )

            try BenchmarkReport.publish(
                BenchmarkReport(
                    scenario: "B4.coinRecyclingReplica",
                    variant: variant,
                    scale: scale,
                    summaries: [
                        voucherSaves.summary(),
                        registration.summary(),
                        statusSaves.summary(),
                        readers.summary(),
                        wall.summary()
                    ],
                    counters: counters
                )
            )
        }
    }
}
