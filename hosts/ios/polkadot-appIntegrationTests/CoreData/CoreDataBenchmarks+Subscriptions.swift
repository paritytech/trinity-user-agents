import Foundation
import Testing

@testable import polkadot_app

extension CoreDataBenchmarks {
    @Suite("Subscriptions", .serialized)
    struct Subscriptions {
        let scale = BenchmarkScale.default

        /// Twenty live snapshot subscriptions, then single-row inserts: how long a save takes with
        /// every subscriber re-mapping behind it, and how long until the snapshot reaches a consumer.
        @Test("B3 subscription fan-out", .timeLimit(.minutes(5)), arguments: StackVariant.supported)
        func subscriptionFanOut(variant: StackVariant) async throws {
            let facade = try BenchmarkStorageFacade(variant: variant)
            let seeder = BenchmarkSeeder(facade: facade)

            let chats = max(6, scale.subscriptionRows / scale.messagesPerChat)
            let chatIds = try await seeder.seedChats(count: chats, messagesPerChat: scale.messagesPerChat)
            _ = try await seeder.seedCoins(count: 50)
            try await seeder.seedVouchers(count: 50, startIndex: 10_000)
            let seededMessages = chats * scale.messagesPerChat

            let subscriptions = ProductionFloorSubscriptions(facade: facade, chatIds: chatIds)
            subscriptions.start()

            let tracker = subscriptions.tracker
            let unfiltered = ProductionFloorSubscriptions.unfilteredMessages
            await tracker.waitForSize(subscription: unfiltered, atLeast: seededMessages)

            let saves = LatencyRecorder(scenario: "B3.save", variant: variant)
            let delivery = LatencyRecorder(scenario: "B3.deliver", variant: variant)

            for index in 0 ..< scale.subscriptionSaves {
                let mark = ContinuousClock.now
                try await saves.time { try await seeder.insertMessage(index: index, chatId: chatIds[0]) }
                await tracker.waitForSize(subscription: unfiltered, atLeast: seededMessages + index + 1)
                delivery.record(since: mark)
            }

            subscriptions.stop()

            try BenchmarkReport.publish(
                BenchmarkReport(
                    scenario: "B3.subscriptionFanOut",
                    variant: variant,
                    scale: scale,
                    summaries: [saves.summary(), delivery.summary()],
                    counters: subscriptions.counters
                )
            )
        }
    }
}
