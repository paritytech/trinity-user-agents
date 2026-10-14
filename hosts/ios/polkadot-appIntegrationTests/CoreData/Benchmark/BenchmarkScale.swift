import Foundation

/// Every size a scenario depends on, in one place. Reports embed the scale they ran with, so a
/// reduced run is never mistaken for a faster one.
struct BenchmarkScale: Codable, Sendable {
    var chats = 200
    var messagesPerChat = 50
    var readerTasks = 8
    var fetchesPerReader = 200
    var writerBatches = 50
    var rowsPerBatch = 100
    var subscriptionRows = 2_000
    var subscriptionSaves = 100
    var recyclingChats = 100
    var recyclingCoins = 500
    var readYourWritesIterations = 1_000
    var spikeSeedRows = 5_000
    var spikeWriteRows = 20_000
    var spikeReaderFetches = 200

    static let `default` = BenchmarkScale()
}
