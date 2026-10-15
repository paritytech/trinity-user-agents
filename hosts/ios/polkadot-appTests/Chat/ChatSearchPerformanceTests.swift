import CoreData
import Foundation
import Operation_iOS
import Testing
@testable import polkadot_app

/// B8 (04-TESTING §5): the in-room search of a host-store room with 50,000 messages gives its first results in under
/// 300 ms. The room is in an SQLite store on disk, as on a device.
///
/// The test runs only when the test process has `TRUAPI_CHAT_PERF=1` (`TEST_RUNNER_TRUAPI_CHAT_PERF=1` on the
/// `xcodebuild` command line). It writes `[perf] B8` lines, also to the file that `TRUAPI_CHAT_PERF_OUT` names.
final class ChatSearchPerformanceTests {
    private static let measures = ProcessInfo.processInfo.environment["TRUAPI_CHAT_PERF"] == "1"
    private static let messageCount = 50_000
    private static let target: Duration = .milliseconds(300)

    private let directory = URL.temporaryDirectory.appending(path: "chat-search-perf-\(UUID().uuidString)")
    private let chatId = Chat.Id.chatExtension("game.dot", roomId: "big")

    deinit {
        try? FileManager.default.removeItem(at: directory)
    }

    @Test(.enabled(if: measures)) func b8TheFirstResultsOfA50000MessageRoomComeInUnder300Milliseconds() async throws {
        let databaseService = try makeDatabase()
        try await fill(databaseService)
        let service = ChatSearchService(databaseService: databaseService)

        // The first search of the run also checks for rows without a search text.
        let cold = try await measure { try await service.searchMessages(matching: "Needle", in: chatId, after: nil) }

        var results: [String: (Duration, Int)] = [:]
        for (name, query) in [("newest", "message"), ("rare", "needle"), ("none", "zzzz"), ("diacritics", "CAFÉ")] {
            var times: [Duration] = []
            var count = 0
            for _ in 0 ..< 5 {
                let (time, page) = try await timed {
                    try await service.searchMessages(matching: query, in: chatId, after: nil)
                }
                times.append(time)
                count = page.hits.count
            }
            results[name] = (times.sorted()[times.count / 2], count)
        }

        report("[perf] B8 messages=\(Self.messageCount) cold-first-page ms=\(Self.milliseconds(cold))")
        for name in ["newest", "rare", "none", "diacritics"] {
            guard let (median, hits) = results[name] else { continue }
            report("[perf] B8 query=\(name) hits=\(hits) median ms=\(Self.milliseconds(median))")
        }

        #expect(results["rare"]?.1 == 50)
        #expect(results["diacritics"]?.1 == 50)
        #expect(cold < Self.target)
        #expect(results.values.allSatisfy { $0.0 < Self.target })
    }
}

private extension ChatSearchPerformanceTests {
    func makeDatabase() throws -> CoreDataServiceProtocol {
        let bundle = Bundle.main
        let modelName = UserStorageParams.modelVersion.rawValue
        let modelURL = try #require(
            bundle.url(forResource: modelName, withExtension: "omo", subdirectory: UserStorageParams.modelDirectory)
                ?? bundle.url(
                    forResource: modelName,
                    withExtension: "mom",
                    subdirectory: UserStorageParams.modelDirectory
                )
        )
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)

        let settings = CoreDataPersistentSettings(databaseDirectory: directory, databaseName: "b8.sqlite")
        return CoreDataService(configuration: CoreDataServiceConfiguration(
            modelURL: modelURL,
            storageType: .persistent(settings: settings)
        ))
    }

    /// One room with 50,000 text messages, each with the search text that the mapper writes. Every 1,000th message
    /// holds "needle" and every 1,000th "Café".
    func fill(_ databaseService: CoreDataServiceProtocol) async throws {
        let identifier = chatId.rawRepresentation
        let batch = 5_000

        for start in stride(from: 0, to: Self.messageCount, by: batch) {
            try await databaseService.performWrite { context in
                let chat = try context.fetch(CDChat.fetchRequest()).first ?? {
                    let created = CDChat(context: context)
                    created.identifier = identifier
                    return created
                }()

                for index in start ..< start + batch {
                    let text = Self.text(of: index)
                    let content = CDMessageContent(context: context)
                    content.data = try Chat.LocalMessage.Content.text(text).scaleEncoded()

                    let message = CDChatMessage(context: context)
                    message.messageId = String(format: "m%05d", index)
                    message.chat = chat
                    message.content = content
                    message.contentType = Int16(Chat.LocalMessage.Content.ContentType.text.rawValue)
                    message.status = Chat.LocalMessage.Status.incoming(.seen).rawValue
                    message.timestamp = Int64(1_700_000_000_000 + index * 1_000)
                    message.order = Int64(index)
                    message.searchableText = ChatSearchText.stored(for: .text(text))
                }
            }
        }
    }

    static func text(of index: Int) -> String {
        switch index % 1_000 {
        case 17: "Message \(index) has the needle in it"
        case 501: "Message \(index) at the café"
        default: "Message \(index) about topic \(index % 97) with some more words in the line"
        }
    }

    func measure(_ body: () async throws -> ChatSearchPage) async throws -> Duration {
        try await timed(body).0
    }

    func timed(_ body: () async throws -> ChatSearchPage) async throws -> (Duration, ChatSearchPage) {
        let start = ContinuousClock.now
        let page = try await body()
        return (ContinuousClock.now - start, page)
    }

    static func milliseconds(_ duration: Duration) -> String {
        let value = Double(duration.components.seconds) * 1_000 + Double(duration.components.attoseconds) / 1e15
        return String(format: "%.1f", value)
    }

    /// Prints a line, and adds it to the file of `TRUAPI_CHAT_PERF_OUT`.
    func report(_ line: String) {
        print(line)
        guard let path = ProcessInfo.processInfo.environment["TRUAPI_CHAT_PERF_OUT"],
              let data = (line + "\n").data(using: .utf8) else { return }
        if let handle = FileHandle(forWritingAtPath: path) {
            handle.seekToEndOfFile()
            handle.write(data)
            try? handle.close()
        } else {
            FileManager.default.createFile(atPath: path, contents: data)
        }
    }
}
