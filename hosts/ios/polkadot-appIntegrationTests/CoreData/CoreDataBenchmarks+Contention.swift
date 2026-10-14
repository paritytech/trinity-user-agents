import Foundation
import Testing

@testable import polkadot_app

extension CoreDataBenchmarks {
    @Suite("Contention", .serialized)
    struct Contention {
        let scale = BenchmarkScale.default

        @Test("B1 concurrent reads", .timeLimit(.minutes(5)), arguments: StackVariant.supported)
        func concurrentReads(variant: StackVariant) async throws {
            let facade = try BenchmarkStorageFacade(variant: variant)
            let seeder = BenchmarkSeeder(facade: facade)
            let chatIds = try await seeder.seedChats(count: scale.chats, messagesPerChat: scale.messagesPerChat)

            let readers = LatencyRecorder(scenario: "B1.read", variant: variant)

            try await seeder.runReaderLoad(
                chatIds: chatIds,
                tasks: scale.readerTasks,
                fetchesPerTask: scale.fetchesPerReader,
                recorder: readers
            )

            try BenchmarkReport.publish(
                BenchmarkReport(
                    scenario: "B1.concurrentReads",
                    variant: variant,
                    scale: scale,
                    summaries: [readers.summary()]
                )
            )
        }

        @Test("B2 reads under write load", .timeLimit(.minutes(5)), arguments: StackVariant.supported)
        func readsUnderWriteLoad(variant: StackVariant) async throws {
            let facade = try BenchmarkStorageFacade(variant: variant)
            let seeder = BenchmarkSeeder(facade: facade)
            let chatIds = try await seeder.seedChats(count: scale.chats, messagesPerChat: scale.messagesPerChat)

            let readers = LatencyRecorder(scenario: "B2.read", variant: variant)
            let writes = LatencyRecorder(scenario: "B2.write", variant: variant)

            try await withThrowingTaskGroup(of: Void.self) { group in
                group.addTask {
                    try await seeder.upsertMessages(
                        batches: scale.writerBatches,
                        rowsPerBatch: scale.rowsPerBatch,
                        chatIds: chatIds,
                        recorder: writes
                    )
                }

                group.addTask {
                    try await seeder.runReaderLoad(
                        chatIds: chatIds,
                        tasks: scale.readerTasks,
                        fetchesPerTask: scale.fetchesPerReader,
                        recorder: readers
                    )
                }

                try await group.waitForAll()
            }

            try BenchmarkReport.publish(
                BenchmarkReport(
                    scenario: "B2.readsUnderWriteLoad",
                    variant: variant,
                    scale: scale,
                    summaries: [readers.summary(), writes.summary()]
                )
            )
        }
    }
}
