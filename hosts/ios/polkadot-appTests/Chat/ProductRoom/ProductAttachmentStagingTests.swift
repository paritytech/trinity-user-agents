import Foundation
import os
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// The staged attachments of product rooms (P10.3, 04-TESTING S57 to S60).
struct ProductAttachmentStagingTests {
    private static let mebibyte = 1_048_576
    private static let key = Foundation.Data(repeating: 7, count: 32)

    // MARK: - Routes

    @Test func anAttachmentAboveTheInlineLimitIsStagedWhenTheRoomAllowsIt() throws {
        let bytes = Foundation.Data(count: 3 * Self.mebibyte)
        let policy = ProductAttachmentStagingPolicy(
            allowed: true,
            maxBytes: ProductAttachmentStagingPolicy.coreMaxBytes
        )

        let route = try ProductAttachmentEncoder.route(.file(bytes, name: "report.pdf"), staging: policy)

        #expect(route == .staged(ProductAttachment(
            kind: .file(name: "report.pdf"),
            mimeType: "application/pdf",
            data: bytes
        )))
    }

    @Test func aStagedImageKeepsItsBytesAndItsFormat() throws {
        let png = Self.noisePNG(side: 700)
        #expect(png.count > ProductAttachmentEncoder.inlineLimit)
        let policy = ProductAttachmentStagingPolicy(
            allowed: true,
            maxBytes: ProductAttachmentStagingPolicy.coreMaxBytes
        )

        let route = try ProductAttachmentEncoder.route(.image(png), staging: policy)

        #expect(route == .staged(ProductAttachment(kind: .image, mimeType: "image/png", data: png)))
    }

    @Test func aSmallAttachmentStaysInlineInARoomThatStages() throws {
        let policy = ProductAttachmentStagingPolicy(
            allowed: true,
            maxBytes: ProductAttachmentStagingPolicy.coreMaxBytes
        )

        let route = try ProductAttachmentEncoder.route(.file(Foundation.Data([1, 2]), name: "a.txt"), staging: policy)

        guard case .inline = route else {
            Issue.record("an attachment within 1 MiB is inline")
            return
        }
    }

    /// S58: above the staging limit, or with no declaration, the attachment is refused before any upload.
    @Test func anAttachmentAboveTheLimitOrWithoutTheFeatureIsRefused() {
        let large = Foundation.Data(count: 8 * Self.mebibyte + 1)
        let staging = ProductAttachmentStagingPolicy(allowed: true, maxBytes: 8 * Self.mebibyte)

        #expect(throws: ProductAttachmentError.aboveStagingLimit(bytes: large.count, limit: 8 * Self.mebibyte)) {
            try ProductAttachmentEncoder.route(.video(large, mimeType: "video/mp4"), staging: staging)
        }
        #expect(throws: ProductAttachmentError.tooLarge(bytes: 2 * Self.mebibyte)) {
            try ProductAttachmentEncoder.route(
                .file(Foundation.Data(count: 2 * Self.mebibyte), name: "a.bin"),
                staging: .inlineOnly
            )
        }
    }

    // MARK: - Stager

    /// S57: the core stages the bytes, and the product receives the preimage key with the facts of the media.
    @Test func theStagerStagesAndSendsTheMessageWithThePreimageKey() async throws {
        let harness = try StagerHarness()
        let created = ChatMessage(
            messageId: "p1",
            origin: ChatMessageOrigin(participantId: productUserParticipantId),
            timestamp: 1,
            body: .text(text: "ok"),
            status: .sent,
            replyTo: nil,
            editedAt: nil,
            reactions: []
        )
        harness.delivery.answer = .success(created)
        let events = await harness.stager.events()

        let entry = try await harness.stager.send(
            ProductAttachment(
                kind: .file(name: "a.pdf"),
                mimeType: "application/pdf",
                data: Foundation.Data(count: 1_500_000)
            ),
            productId: "game.dot",
            roomId: "lobby",
            store: .product
        )
        await harness.stager.waitUntilIdle(entry.id)

        let call = StagingCall(productId: "game.dot", roomId: "lobby", mimeType: "application/pdf", size: 1_500_000)
        #expect(harness.staging.calls == [call])
        #expect(harness.staging.holds == ["hold game.dot", "release game.dot"])
        guard case let .richText(nil, media)? = harness.delivery.delivered.first,
              case let .file(.preimage(key), meta, fileName)? = media.first else {
            Issue.record("the product receives the file as a preimage")
            return
        }
        #expect(key == Self.key)
        #expect(fileName == "a.pdf")
        #expect(meta.sizeBytes == 1_500_000)
        #expect(harness.outbox.entries().isEmpty)
        #expect(await Self.collect(events, count: 2).last == .delivered(
            entry.withKey(Self.key),
            created: created,
            body: harness.delivery.delivered[0]
        ))
    }

    /// S59: a failed submit keeps the entry. Retry stages again and succeeds; Delete removes the kept bytes.
    @Test func aFailedSubmitKeepsTheEntryUntilRetryOrDelete() async throws {
        let harness = try StagerHarness()
        harness.staging.failures = [.submitFailed(reason: "the chain refused the transaction")]

        let entry = try await harness.stager.send(Self.image(), productId: "game.dot", roomId: "lobby", store: .host)
        await harness.stager.waitUntilIdle(entry.id)

        #expect(harness.outbox.entry(entry.id)?.isFailed == true)
        #expect(harness.outbox.bytes(of: entry.id) != nil)
        #expect(harness.delivery.shown.last?.isFailed == true)
        #expect(harness.delivery.delivered.isEmpty)

        #expect(await harness.stager.retry(entry.id))
        await harness.stager.waitUntilIdle(entry.id)
        #expect(harness.staging.calls.count == 2)
        #expect(harness.delivery.delivered.count == 1)
        #expect(harness.outbox.entry(entry.id) == nil)

        harness.staging.failures = [.allowanceUnavailable(reason: "none")]
        let second = try await harness.stager.send(Self.image(), productId: "game.dot", roomId: "lobby", store: .host)
        await harness.stager.waitUntilIdle(second.id)
        #expect(await harness.stager.delete(second.id))
        #expect(harness.outbox.entry(second.id) == nil)
        #expect(harness.outbox.bytes(of: second.id) == nil)
        #expect(harness.delivery.removed == [second.id])
    }

    /// A message that the product did not get goes again with the key that the core gave, without a new upload.
    @Test func aRetryAfterAFailedDeliveryDoesNotStageAgain() async throws {
        let harness = try StagerHarness()
        harness.delivery.answer = .failure(.notDelivered(reason: "timed out"))

        let entry = try await harness.stager.send(Self.image(), productId: "game.dot", roomId: "lobby", store: .product)
        await harness.stager.waitUntilIdle(entry.id)
        #expect(harness.outbox.entry(entry.id)?.key == Self.key)

        harness.delivery.answer = .success(nil)
        #expect(await harness.stager.retry(entry.id))
        await harness.stager.waitUntilIdle(entry.id)

        #expect(harness.staging.calls.count == 1)
        #expect(harness.delivery.delivered.count == 2)
        #expect(harness.outbox.entry(entry.id) == nil)
    }

    /// S58: a refused `PreimageSubmit` or the live Bulletin bound ends the attachment, and nothing is sent.
    @Test(arguments: [ProductStageFailure.permissionDenied, .tooLarge(maxBytes: 2_097_152)])
    func aFinalRefusalRemovesTheAttachment(failure: ProductStageFailure) async throws {
        let harness = try StagerHarness()
        harness.staging.failures = [failure]
        let events = await harness.stager.events()

        let entry = try await harness.stager.send(Self.image(), productId: "game.dot", roomId: "lobby", store: .host)
        await harness.stager.waitUntilIdle(entry.id)

        #expect(harness.delivery.delivered.isEmpty)
        #expect(harness.delivery.removed == [entry.id])
        #expect(harness.outbox.entries().isEmpty)
        #expect(await Self.collect(events, count: 2).last == .removed(entry, failure: failure))
        let limit = await harness.stager.stagingLimit(productId: "game.dot")
        #expect(limit == (failure == .permissionDenied ? ProductAttachmentStagingPolicy.coreMaxBytes : 2_097_152))
    }

    /// An entry that a closed app left in the `sending` state shows as failed, so the user can retry it.
    @Test func anInterruptedEntryIsFailedAfterARestart() async throws {
        let harness = try StagerHarness()
        let entry = ProductOutboxEntry(
            id: "staged-1",
            productId: "game.dot",
            roomId: "lobby",
            store: .host,
            kind: .image,
            mimeType: "image/png",
            sizeBytes: 3,
            width: nil,
            height: nil,
            placeholder: nil,
            createdAt: 1,
            key: nil,
            state: .sending
        )
        try harness.outbox.save(entry, bytes: Foundation.Data([1, 2, 3]))

        await harness.stager.resumeInterrupted()
        let entries = await harness.stager.entries(productId: "game.dot", roomId: "lobby")

        #expect(entries.map(\.isFailed) == [true])
        #expect(harness.delivery.shown.map(\.state) == [.failed(reason: "interrupted")])
        #expect(await harness.stager.owns("staged-1"))
    }

    // MARK: - Window of a product-source room

    @Test func theWindowShowsAnEntryAndReplacesItWithTheMessageOfTheProduct() async throws {
        let harness = try StagerHarness()
        harness.staging.gate = AsyncGate()
        let created = ChatMessage(
            messageId: "p9",
            origin: ChatMessageOrigin(participantId: productUserParticipantId),
            timestamp: 2,
            body: .text(text: "media"),
            status: .sent,
            replyTo: nil,
            editedAt: nil,
            reactions: []
        )
        harness.delivery.answer = .success(created)
        let source = ProductMessageSource(
            chatId: .chatExtension("game.dot", roomId: "lobby"),
            productId: "game.dot",
            roomId: "lobby",
            querying: ScriptedProductChatQuerying(),
            cache: ProductMessageCache(),
            settings: .init(retryDelay: .milliseconds(1)),
            now: { 1 },
            stager: harness.stager,
            logger: StubLogger()
        )
        await source.start()
        await source.waitForFirstPage()

        let entry = try await harness.stager.send(Self.image(), productId: "game.dot", roomId: "lobby", store: .product)
        try await Self.until { await source.snapshot.messages.contains { $0.messageId == entry.id } }
        let waiting = await source.snapshot.messages.first { $0.messageId == entry.id }
        #expect(waiting?.status == .outgoing(.new))

        await harness.staging.gate?.open()
        await harness.stager.waitUntilIdle(entry.id)
        try await Self.until { await source.snapshot.messages.contains { $0.messageId == "p9" } }
        #expect(await !source.snapshot.messages.contains { $0.messageId == entry.id })
    }
}

// MARK: - Room screen

@MainActor
struct ProductAttachmentStagingPresenterTests {
    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()
    private let view = RecordingProductRoomView()

    /// S60: the public-storage notice shows once per product. A refusal keeps the attachment unsent.
    @Test func theNoticeShowsOncePerProduct() async throws {
        let harness = try StagerHarness()
        let defaults = try #require(UserDefaults(suiteName: "staging-notice-\(UUID().uuidString)"))
        let presenter = try makePresenter(harness: harness, defaults: defaults)

        wireframe.acceptsNotice = false
        presenter.sendAttachment(.file(Foundation.Data(count: 1_200_000), name: "a.bin"))
        try await Task.sleep(for: .milliseconds(50))
        #expect(wireframe.notices.count == 1)
        #expect(harness.outbox.entries().isEmpty)

        wireframe.acceptsNotice = true
        presenter.sendAttachment(.file(Foundation.Data(count: 1_200_000), name: "b.bin"))
        presenter.sendAttachment(.file(Foundation.Data(count: 1_200_000), name: "c.bin"))
        try await StagerHarness.until { harness.staging.calls.count == 2 }

        #expect(wireframe.notices.count == 2)
        #expect(view.failures.isEmpty)
    }

    /// Without the declaration of the room, a large attachment keeps the inline limit and nothing is staged.
    @Test func aRoomWithoutTheFeatureRefusesALargeAttachment() async throws {
        let harness = try StagerHarness()
        let presenter = try makePresenter(harness: harness, declares: false)

        presenter.sendAttachment(.file(Foundation.Data(count: 1_200_000), name: "a.bin"))
        try await Task.sleep(for: .milliseconds(50))

        #expect(view.failures.count == 1)
        #expect(wireframe.notices.isEmpty)
        #expect(harness.staging.calls.isEmpty)
    }

    private func makePresenter(
        harness: StagerHarness,
        declares: Bool = true,
        defaults: UserDefaults = UserDefaults(suiteName: "staging-notice-\(UUID().uuidString)") ?? .standard
    ) throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: ProductRoomTestData.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            logger: StubLogger()
        ))
        presenter.view = view
        presenter.staging = ProductRoomStaging(
            stager: harness.stager,
            notice: ProductStagingNotice(defaults: defaults),
            hostListsFeature: true
        )
        var policy = ProductRoomTestData.policy(ProductRoomTestData.config())
        policy.features = Chat.RoomFeatures(
            preimageAttachments: declares,
            search: false,
            calls: nil,
            threadOf: nil,
            readAloud: false
        )
        presenter.update(policy: policy, messages: [])
        return presenter
    }
}

private extension ProductAttachmentStagingTests {
    static func image() -> ProductAttachment {
        ProductAttachment(kind: .image, mimeType: "image/png", data: Foundation.Data(count: 1_200_000))
    }

    static func until(_ condition: () async -> Bool) async throws {
        try await StagerHarness.until(condition)
    }

    static func collect(_ events: AsyncStream<ProductOutboxEvent>, count: Int) async -> [ProductOutboxEvent] {
        var collected: [ProductOutboxEvent] = []
        for await event in events {
            collected.append(event)
            if collected.count == count { break }
        }
        return collected
    }

    /// An image of random pixels. Its PNG does not compress, so it is larger than the inline limit.
    static func noisePNG(side: Int) -> Foundation.Data {
        var generator = SystemRandomNumberGenerator()
        var pixels = [UInt8](repeating: 255, count: side * side * 4)
        for index in pixels.indices where index % 4 != 3 {
            pixels[index] = UInt8.random(in: 0 ... 255, using: &generator)
        }
        let image = pixels.withUnsafeMutableBytes { buffer in
            CGContext(
                data: buffer.baseAddress,
                width: side,
                height: side,
                bitsPerComponent: 8,
                bytesPerRow: side * 4,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
            )?.makeImage()
        }
        return image.flatMap { UIImage(cgImage: $0).pngData() } ?? Foundation.Data()
    }
}

private extension ProductOutboxEntry {
    func withKey(_ key: Foundation.Data) -> ProductOutboxEntry {
        var entry = self
        entry.key = key
        return entry
    }
}
