import Foundation
import Testing

@testable import polkadot_app

@Suite("ChatMessageOrderAllocator")
struct ChatMessageOrderAllocatorTests {
    @Test("fresh file starts at one")
    func freshFileStartsAtOne() throws {
        let tempURL = FileManager.default.temporaryDirectory.appendingPathComponent(
            "order-\(UUID().uuidString)"
        )
        defer { try? FileManager.default.removeItem(at: tempURL) }

        let allocator = FileChatMessageOrderAllocator(fileURL: tempURL)

        let values = try (0 ..< 3).map { _ in try allocator.nextOrder { 0 } }

        #expect(values == [1, 2, 3])
    }

    @Test("empty counter starts above floor")
    func emptyCounterStartsAboveFloor() throws {
        let tempURL = FileManager.default.temporaryDirectory.appendingPathComponent(
            "order-\(UUID().uuidString)"
        )
        defer { try? FileManager.default.removeItem(at: tempURL) }

        let allocator = FileChatMessageOrderAllocator(fileURL: tempURL)

        #expect(try allocator.nextOrder { 41 } == 42)
    }

    @Test("floor is not evaluated when counter exists")
    func floorNotEvaluatedWhenCounterExists() throws {
        let tempURL = FileManager.default.temporaryDirectory.appendingPathComponent(
            "order-\(UUID().uuidString)"
        )
        defer { try? FileManager.default.removeItem(at: tempURL) }

        let allocator = FileChatMessageOrderAllocator(fileURL: tempURL)

        #expect(try allocator.nextOrder { 41 } == 42)

        var floorCalled = false
        let result = try allocator.nextOrder {
            floorCalled = true
            return 0
        }

        #expect(result == 43)
        #expect(!floorCalled)
    }

    @Test("corrupted counter recovers from floor")
    func corruptedCounterRecoversFromFloor() throws {
        let tempURL = FileManager.default.temporaryDirectory.appendingPathComponent(
            "order-\(UUID().uuidString)"
        )
        defer { try? FileManager.default.removeItem(at: tempURL) }

        try Data([1, 2, 3]).write(to: tempURL)

        let allocator = FileChatMessageOrderAllocator(fileURL: tempURL)

        #expect(try allocator.nextOrder { 41 } == 42)
        #expect(try Data(contentsOf: tempURL).count == 8)
    }

    @Test("two allocators on one file never repeat a value")
    func twoAllocatorsNeverRepeat() async throws {
        let tempURL = FileManager.default.temporaryDirectory.appendingPathComponent(
            "order-\(UUID().uuidString)"
        )
        defer { try? FileManager.default.removeItem(at: tempURL) }

        let allocator1 = FileChatMessageOrderAllocator(fileURL: tempURL)
        let allocator2 = FileChatMessageOrderAllocator(fileURL: tempURL)

        let lock = NSLock()
        var values: [UInt64] = []

        try await withThrowingTaskGroup(of: Void.self) { group in
            for taskIndex in 0 ..< 8 {
                group.addTask {
                    for _ in 0 ..< 200 {
                        let allocator = taskIndex % 2 == 0 ? allocator1 : allocator2
                        let value = try allocator.nextOrder { 0 }
                        lock.withLock {
                            values.append(value)
                        }
                    }
                }
            }

            try await group.waitForAll()
        }

        #expect(Set(values).count == values.count)
        #expect(Set(values) == Set(1 ... 1_600))
    }
}
