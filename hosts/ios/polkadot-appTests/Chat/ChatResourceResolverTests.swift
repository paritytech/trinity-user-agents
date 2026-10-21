import AsyncExtensions
import Foundation
import os
import Testing
import UIKit
@testable import polkadot_app

final class ChatResourceResolverTests {
    private let directory = URL.temporaryDirectory.appending(path: "chat-resources-\(UUID().uuidString)")
    private let preimages = CountingPreimages()
    private let fetches = OSAllocatedUnfairLock(initialState: 0)

    deinit {
        try? FileManager.default.removeItem(at: directory)
    }

    @Test func aV1SourceIsAnInlineImageOrAnHttpsAddress() throws {
        let png = try pictureData()

        #expect(ChatResource(source: "data:image/png;base64,\(png.base64EncodedString())")
            == .inline(mimeType: "image/png", data: png))
        #expect(try ChatResource(source: "https://example.com/a.png")
            == .address(#require(URL(string: "https://example.com/a.png"))))
        #expect(ChatResource(source: "http://example.com/a.png") == nil)
        #expect(ChatResource(source: "javascript:alert(1)") == nil)
    }

    @Test func inlineBytesNeedNoFetch() async throws {
        let png = try pictureData()
        let resolver = makeResolver()

        let image = await resolver.image(for: .inline(mimeType: "image/png", data: png))

        #expect(image?.size == CGSize(width: 2, height: 2))
        #expect(preimages.lookups == 0)
        #expect(fetches.withLock { $0 } == 0)
    }

    /// The node drops an acked file, so the resolver keeps the bytes before the ack, and later draws read them from
    /// the store of opened files.
    @Test func aHopFileIsOpenedOnceKeptAndOnlyThenAcked() async throws {
        let hopFiles = RecordingHopFiles(bytes: Data([4, 5, 6]))
        let hop = ChatHopResource(
            identifier: Data(repeating: 1, count: 32),
            ticket: Data(repeating: 2, count: 32),
            node: "wss://hop.example",
            size: 3
        )
        let resolver = makeResolver(hopFiles: hopFiles)

        #expect(await resolver.data(for: .hop(hop)) == Data([4, 5, 6]))
        #expect(await makeResolver(hopFiles: hopFiles).data(for: .hop(hop)) == Data([4, 5, 6]))
        #expect(hopFiles.calls == ["open", "ack"])
        #expect(ChatResource(source: ChatResource.hopSource(hop)) == .hop(hop))
    }

    @Test func aHopFileTheCoreCannotOpenIsNotAckedAndIsTriedAgain() async {
        let hopFiles = RecordingHopFiles(bytes: nil)
        let hop = ChatHopResource(identifier: Data(count: 32), ticket: Data(count: 32), node: "wss://hop.example", size: 1)

        #expect(await makeResolver(hopFiles: hopFiles).data(for: .hop(hop)) == nil)
        #expect(await makeResolver(hopFiles: hopFiles).data(for: .hop(hop)) == nil)
        #expect(hopFiles.calls == ["open", "open"])
    }

    @Test func aPreimageIsReadOnceAndThenFromTheDiskCache() async throws {
        let png = try pictureData()
        preimages.value = png
        let key = Data(repeating: 7, count: 32)

        #expect(await makeResolver().data(for: .preimage(key: key)) == png)
        #expect(await makeResolver().data(for: .preimage(key: key)) == png)
        #expect(preimages.lookups == 1)
    }

    @Test func aMissingPreimageIsNotCached() async throws {
        let key = Data(repeating: 1, count: 32)
        let resolver = makeResolver()

        #expect(await resolver.data(for: .preimage(key: key)) == nil)

        preimages.value = Data([1, 2, 3])
        #expect(await resolver.data(for: .preimage(key: key)) == Data([1, 2, 3]))
        #expect(preimages.lookups == 2)
    }

    @Test func anHttpsAddressIsFetchedOnce() async throws {
        let url = try #require(URL(string: "https://example.com/a.png"))

        #expect(await makeResolver().data(for: .address(url)) == Data([9]))
        #expect(await makeResolver().data(for: .address(url)) == Data([9]))
        #expect(fetches.withLock { $0 } == 1)
    }

    @Test func theDiskCacheRemovesTheOldestFilesPastItsBound() async throws {
        let cache = ChatResourceDiskCache(directory: directory, maxBytes: 10)

        await cache.store(Data(repeating: 1, count: 6), for: "old")
        try await Task.sleep(for: .milliseconds(20))
        await cache.store(Data(repeating: 2, count: 6), for: "new")

        #expect(await cache.data(for: "old") == nil)
        #expect(await cache.data(for: "new") == Data(repeating: 2, count: 6))
    }

    @Test func anHttpsRoomIconLoadsAsAResource() {
        let icon = roomPeerMetadata(icon: "https://example.com/room.png").icon

        #expect(icon == .resource("https://example.com/room.png"))
    }

    @Test func anInlineRoomIconDrawsAtOnce() throws {
        let png = try pictureData()
        let icon = roomPeerMetadata(icon: "data:image/png;base64,\(png.base64EncodedString())").icon

        #expect(icon == .image(png))
    }
}

private extension ChatResourceResolverTests {
    func makeResolver(hopFiles: ChatHopFileOpening? = nil) -> ChatResourceResolver {
        ChatResourceResolver(
            preimages: preimages,
            cache: ChatResourceDiskCache(directory: directory, maxBytes: 1_024 * 1_024),
            sealedFiles: ChatResourceDiskCache(directory: directory.appending(path: "opened"), maxBytes: 1_024 * 1_024),
            hopFiles: hopFiles,
            fetch: { [fetches] _ in
                fetches.withLock { $0 += 1 }
                return Data([9])
            }
        )
    }

    func pictureData() throws -> Data {
        let file = try writeTestPicture(size: CGSize(width: 2, height: 2))
        defer { try? FileManager.default.removeItem(at: file) }
        return try Data(contentsOf: file)
    }

    func roomPeerMetadata(icon: String) -> Chat.PeerMetadata {
        let room = Chat.LocalModel(
            peer: .chatExtension("product.dot", roomId: "room"),
            message: nil,
            unreadDisplayMessageCount: 0,
            hasIncomingReaction: false,
            createdAt: nil,
            roomMetadata: Chat.RoomMetadata(chatRelativeId: "room", name: nil, icon: icon),
            roomFooter: nil
        )
        return room.peerMetadata(using: EmptyRegistry())
    }
}

private final class RecordingHopFiles: ChatHopFileOpening, @unchecked Sendable {
    private let bytes: Data?
    private let state = OSAllocatedUnfairLock(initialState: [String]())

    init(bytes: Data?) {
        self.bytes = bytes
    }

    var calls: [String] { state.withLock { $0 } }

    func open(_: ChatHopResource) async throws -> ChatHopFile {
        state.withLock { $0.append("open") }
        guard let bytes else { throw URLError(.cannotConnectToHost) }
        return ChatHopFile(bytes: bytes, entries: [Data(repeating: 1, count: 32)])
    }

    func ack(_: ChatHopResource, entries _: [Data]) async throws {
        state.withLock { $0.append("ack") }
    }
}

private final class CountingPreimages: TrUAPIPreimageLookuping, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock(initialState: (value: Data?.none, lookups: 0))

    var value: Data? {
        get { state.withLock { $0.value } }
        set { state.withLock { $0.value = newValue } }
    }

    var lookups: Int { state.withLock { $0.lookups } }

    func lookup(key _: Data) async -> Data? {
        state.withLock { state in
            state.lookups += 1
            return state.value
        }
    }
}

private struct EmptyRegistry: ChatExtensionsRegistering {
    var onChangeStream: AnyAsyncSequence<ChatExtensionRegistryChange> {
        AsyncStream<ChatExtensionRegistryChange> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func discover() {}
    func getExtensions(for _: Chat.Id) -> [ChatExtending] { [] }
    func getChatExtensionBot(for _: ChatExtension.Id) -> ChatExtensionBotProtocol? { nil }
    func getWidgetProviders() -> [ChatExtensionWidgetProvider] { [] }
}
