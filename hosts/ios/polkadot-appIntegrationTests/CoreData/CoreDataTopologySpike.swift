import CoreData
import Foundation
import Testing

@testable import polkadot_app

/// S1: raw Core Data, no Operation-iOS. Readers as children of the writer versus siblings on the
/// coordinator with `automaticallyMergesChangesFromParent`, measured while a long write transaction runs.
@Suite("CoreDataTopologySpike", .serialized)
struct CoreDataTopologySpike {
    enum Topology: String, CaseIterable, Sendable {
        case nestedChild
        case siblingAutoMerge
    }

    let scale = BenchmarkScale.default

    @Test("S1 reader latency during a long write", .timeLimit(.minutes(5)), arguments: Topology.allCases)
    func readerLatencyDuringLongWrite(topology: Topology) async throws {
        let stack = try RawStack()
        defer { stack.tearDown() }

        try stack.seed(rows: scale.spikeSeedRows)
        let reader = stack.makeReader(topology)

        let reads = LatencyRecorder(scenario: "S1.\(topology.rawValue).read", variant: .serial)
        let write = LatencyRecorder(scenario: "S1.\(topology.rawValue).write", variant: .serial)

        async let bulkWrite: Void = stack.write(rows: scale.spikeWriteRows, recorder: write)
        try await Task.sleep(for: .milliseconds(50))

        try await withThrowingTaskGroup(of: Void.self) { group in
            for _ in 0 ..< scale.spikeReaderFetches {
                group.addTask {
                    _ = try await reads.time { try await stack.fetch(limit: 50, in: reader) }
                }
                try await Task.sleep(for: .milliseconds(5))
            }

            try await group.waitForAll()
        }

        try await bulkWrite

        try BenchmarkReport.publish(
            BenchmarkReport(
                scenario: "S1.\(topology.rawValue)",
                variant: .serial,
                scale: scale,
                summaries: [reads.summary(), write.summary()]
            )
        )
    }
}

/// One coordinator, one writer, readers attached per topology. Rows are inserted by key-value coding so
/// the spike depends on the entity's attribute names only.
final class RawStack {
    private static let entityName = "CDChatMessage"

    let coordinator: NSPersistentStoreCoordinator
    let writer: NSManagedObjectContext
    private let directory: URL

    init() throws {
        guard let model = try NSManagedObjectModel(contentsOf: BenchmarkStorageFacade.modelURL()) else {
            throw BenchmarkError.modelNotFound
        }

        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("CoreDataSpike.\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)

        coordinator = NSPersistentStoreCoordinator(managedObjectModel: model)
        try coordinator.addPersistentStore(
            ofType: NSSQLiteStoreType,
            configurationName: nil,
            at: directory.appendingPathComponent("spike.sqlite"),
            options: nil
        )

        writer = NSManagedObjectContext(concurrencyType: .privateQueueConcurrencyType)
        writer.persistentStoreCoordinator = coordinator
        writer.name = "spike.writer"
    }

    func makeReader(_ topology: CoreDataTopologySpike.Topology) -> NSManagedObjectContext {
        let reader = NSManagedObjectContext(concurrencyType: .privateQueueConcurrencyType)
        reader.name = "spike.reader.\(topology.rawValue)"

        switch topology {
        case .nestedChild:
            reader.parent = writer
        case .siblingAutoMerge:
            reader.persistentStoreCoordinator = coordinator
            reader.automaticallyMergesChangesFromParent = true
        }

        return reader
    }

    func seed(rows: Int) throws {
        try writer.performAndWait {
            for index in 0 ..< rows {
                Self.insertMessage(id: "seed-\(index)", index: index, in: writer)
            }
            try writer.save()
        }
    }

    func write(rows: Int, recorder: LatencyRecorder) async throws {
        try await recorder.time {
            try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
                writer.perform {
                    do {
                        for index in 0 ..< rows {
                            Self.insertMessage(id: "bulk-\(index)", index: index, in: self.writer)
                        }
                        try self.writer.save()
                        continuation.resume()
                    } catch {
                        continuation.resume(throwing: error)
                    }
                }
            }
        }
    }

    func fetch(limit: Int, in context: NSManagedObjectContext) async throws -> Int {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Int, Error>) in
            context.perform {
                let request = NSFetchRequest<NSManagedObject>(entityName: Self.entityName)
                request.predicate = NSPredicate(format: "messageId BEGINSWITH %@", "seed-")
                request.fetchLimit = limit

                do {
                    try continuation.resume(returning: context.fetch(request).count)
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    func tearDown() {
        writer.performAndWait {
            for store in coordinator.persistentStores {
                try? coordinator.remove(store)
            }
        }
        try? FileManager.default.removeItem(at: directory)
    }
}

private extension RawStack {
    static func insertMessage(id: String, index: Int, in context: NSManagedObjectContext) {
        let object = NSEntityDescription.insertNewObject(forEntityName: entityName, into: context)
        object.setValue(id, forKey: "messageId")
        object.setValue(Int16(0), forKey: "status")
        object.setValue(Int64(index), forKey: "timestamp")
    }
}
