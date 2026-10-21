import Foundation
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// The attachments of a product room (P6.7, scenario S36).
@MainActor
struct ChatProductAttachmentTests {
    private typealias Data = ProductRoomTestData

    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()
    private let view = RecordingProductRoomView()

    @Test func anImageBecomesAnInlineJPEGWithItsSizeAndBlurHash() throws {
        let attachment = try ProductAttachmentEncoder.image(Self.png(width: 40, height: 30))

        #expect(attachment.kind == .image)
        #expect(attachment.mimeType == "image/jpeg")
        #expect(attachment.data.count <= ProductAttachmentEncoder.inlineLimit)

        guard case let .productBody(.richText(richText)) = attachment.content else {
            Issue.record("an image goes as rich text media")
            return
        }
        let body = try #require(Chat.LocalMessage.Content.ProductBody.richText(richText).userWireBody)
        guard case let .richText(_, media) = body, case let .image(_, meta)? = media.first else {
            Issue.record("the product receives an inline image")
            return
        }
        #expect(meta.mimeType == "image/jpeg")
        #expect(meta.sizeBytes == UInt64(attachment.data.count))
        #expect(meta.width == 40)
        #expect(meta.height == 30)
        #expect(meta.placeholder?.isEmpty == false)
    }

    @Test func aLargeImageIsMadeSmallerUntilItFits() throws {
        let attachment = try ProductAttachmentEncoder.image(Self.noisePNG(side: 1_400))

        #expect(attachment.data.count <= ProductAttachmentEncoder.inlineLimit)
    }

    @Test func aVideoOrAFileOverOneMebibyteIsRefused() {
        let large = Foundation.Data(count: ProductAttachmentEncoder.inlineLimit + 1)

        #expect(throws: ProductAttachmentError.tooLarge(bytes: large.count)) {
            try ProductAttachmentEncoder.video(large, mimeType: "video/mp4")
        }
        #expect(throws: ProductAttachmentError.tooLarge(bytes: large.count)) {
            try ProductAttachmentEncoder.file(large, fileName: "big.pdf")
        }
    }

    @Test func aFileKeepsItsNameAndTheTypeOfItsExtension() throws {
        let attachment = try ProductAttachmentEncoder.file(Foundation.Data([1, 2, 3]), fileName: "sample.pdf")

        #expect(attachment.mimeType == "application/pdf")
        guard case let .productBody(.file(file)) = attachment.content else {
            Issue.record("a file goes as a file body")
            return
        }
        #expect(file.fileName == "sample.pdf")
        #expect(file.sizeBytes == 3)

        guard case let .richText(_, media)? = Chat.LocalMessage.Content.ProductBody.file(file).userWireBody,
              case let .file(_, meta, fileName)? = media.first else {
            Issue.record("the product receives an inline file")
            return
        }
        #expect(fileName == "sample.pdf")
        #expect(meta.sizeBytes == 3)
    }

    /// In a room that the host stores, the product receives the attachment as `MessagePosted`.
    @Test func anAttachmentOfTheUserIsAPostedMessage() throws {
        let attachment = try ProductAttachmentEncoder.file(Foundation.Data([1]), fileName: "a.txt")
        let message = Data.message("u1", content: attachment.content, origin: .user, status: .outgoing(.sent))

        guard case let .attachmentPosted(messageId, _)? = ProductChatUserEvent(userMessage: message) else {
            Issue.record("an attachment is a posted message")
            return
        }
        #expect(messageId == "u1")
    }

    /// In a room that the product keeps, the attachment is a `NewMessage` with its media.
    @Test func anAttachmentIsANewMessageInARoomOfTheProduct() throws {
        let attachment = try ProductAttachmentEncoder.image(Self.png(width: 4, height: 4))

        guard case let .newMessage(.richText(nil, media), nil)? = attachment.content.productUserAction else {
            Issue.record("an attachment is a NewMessage with rich text")
            return
        }
        #expect(media.count == 1)
    }

    @Test func aTooLargeAttachmentShowsAMessageAndSendsNothing() async throws {
        let presenter = try makePresenter()
        var sent: [Chat.LocalMessage.Content] = []
        presenter.sendContent = { sent.append($0) }

        presenter.sendAttachment(.file(Foundation.Data(count: ProductAttachmentEncoder.inlineLimit + 1), name: "a.bin"))
        try await Task.sleep(for: .milliseconds(20))

        #expect(view.failures.count == 1)
        #expect(sent.isEmpty)
    }

    @Test func aSmallAttachmentGoesThroughTheMessageSource() async throws {
        let presenter = try makePresenter()
        var sent: [Chat.LocalMessage.Content] = []
        presenter.sendContent = { sent.append($0) }

        presenter.sendAttachment(.file(Foundation.Data([1, 2]), name: "a.txt"))
        for _ in 0 ..< 100 where sent.isEmpty {
            try await Task.sleep(for: .milliseconds(5))
        }

        #expect(sent.count == 1)
        #expect(view.failures.isEmpty)
    }
}

private extension ChatProductAttachmentTests {
    func makePresenter() throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            logger: StubLogger()
        ))
        presenter.view = view
        presenter.update(policy: Data.policy(Data.config()), messages: [])
        return presenter
    }

    static func png(width: Int, height: Int) -> Foundation.Data {
        let format = UIGraphicsImageRendererFormat.default()
        format.scale = 1
        return UIGraphicsImageRenderer(size: CGSize(width: width, height: height), format: format).pngData { context in
            UIColor.systemTeal.setFill()
            context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        }
    }

    /// An image whose pixels do not compress, so its JPEG is larger than the inline limit at full size.
    static func noisePNG(side: Int) -> Foundation.Data {
        var generator = SystemRandomNumberGenerator()
        let format = UIGraphicsImageRendererFormat.default()
        format.scale = 1
        return UIGraphicsImageRenderer(size: CGSize(width: side, height: side), format: format).pngData { context in
            for row in stride(from: 0, to: side, by: 4) {
                for column in stride(from: 0, to: side, by: 4) {
                    UIColor(
                        red: CGFloat.random(in: 0 ... 1, using: &generator),
                        green: CGFloat.random(in: 0 ... 1, using: &generator),
                        blue: CGFloat.random(in: 0 ... 1, using: &generator),
                        alpha: 1
                    ).setFill()
                    context.fill(CGRect(x: column, y: row, width: 4, height: 4))
                }
            }
        }
    }
}
