import Foundation
import PolkadotUI
import DesignSystem
import SubstrateSdk
import SwiftUI
import Testing
import TrUAPIHost
@testable import polkadot_app

/// P7.2: the text alternative of product media reaches VoiceOver, and product text follows the text size that
/// the user picks.
struct ChatAccessibilityTests {
    private let png = Data([0x89, 0x50, 0x4E, 0x47])

    // MARK: - Media `alt`

    @Test func aHostStoreRoomKeepsTheTextAlternativeOfEachImage() {
        let body = ChatMessageBody.richText(text: "look", media: [image(alt: "A red square"), image(alt: nil)])

        guard case let .productBody(.richText(richText)) = body.localContent else {
            Issue.record("a rich text body must be a product body")
            return
        }

        #expect(richText.media.count == 2)
        #expect(richText.alts == ["A red square", nil])
    }

    @Test func aProductSourceRoomKeepsTheTextAlternativeOfEachImage() {
        let body = ChatMessageBody.richText(text: nil, media: [image(alt: "A chart")])

        guard case let .productBody(.richText(richText)) = body.localContent(replyTo: nil) else {
            Issue.record("a rich text body must be a product body")
            return
        }

        #expect(richText.alts == ["A chart"])
    }

    @Test func theTextAlternativesAreStoredWithTheBody() throws {
        let richText = Chat.LocalMessage.Content.ProductRichText(
            text: "hi",
            media: ["data:image/png;base64,AA==", "https://example.com/a.png"],
            alts: ["First", nil]
        )

        let encoder = ScaleEncoder()
        try richText.encode(scaleEncoder: encoder)
        let decoded = try Chat.LocalMessage.Content.ProductRichText(scaleDecoder: ScaleDecoder(data: encoder.encode()))

        #expect(decoded == richText)
    }

    /// A row of the first layout has no text alternatives after its media.
    @Test func aBodyStoredBeforeTheAlternativesDecodesWithNone() throws {
        let encoder = ScaleEncoder()
        try ScaleOption(value: "hi").encode(scaleEncoder: encoder)
        try ["https://example.com/a.png"].encode(scaleEncoder: encoder)

        let decoded = try Chat.LocalMessage.Content.ProductRichText(scaleDecoder: ScaleDecoder(data: encoder.encode()))

        #expect(decoded.media == ["https://example.com/a.png"])
        #expect(decoded.alts.isEmpty)
        #expect(decoded.alt(at: 0) == nil)
    }

    @Test func theMediaStackGivesVoiceOverTheTextAlternativeOfEachImage() {
        let view = ChatMediaStackView(
            sources: ["a", "b", "c"],
            alts: ["A cat", nil, "  "],
            text: nil,
            loader: ChatImageLoader { _ in nil }
        )

        #expect(view.alt(at: 0) == "A cat")
        #expect(view.alt(at: 1) == nil)
        #expect(view.alt(at: 2) == nil)
        #expect(view.alt(at: 3) == nil)
    }

    // MARK: - Dynamic Type

    @Test func productTextFollowsTheTextSize() {
        let standard = ScaledTypography.font(for: .paragraphLarge, dynamicTypeSize: .large)
        let small = ScaledTypography.font(for: .paragraphLarge, dynamicTypeSize: .xSmall)
        let large = ScaledTypography.font(for: .paragraphLarge, dynamicTypeSize: .accessibility3)

        #expect(standard.pointSize == TypographyStyle.paragraphLarge.resolvedSpec.size)
        #expect(small.pointSize < standard.pointSize)
        #expect(large.pointSize > standard.pointSize * 1.5)
        #expect(ScaledTypography.lineHeight(for: .paragraphLarge, dynamicTypeSize: .accessibility3)
            > TypographyStyle.paragraphLarge.resolvedSpec.lineHeight)
    }
}

private extension ChatAccessibilityTests {
    func image(alt: String?) -> ChatMediaItem {
        .image(
            resource: .inline(mimeType: "image/png", bytes: png),
            meta: ChatMediaMeta(
                mimeType: "image/png",
                sizeBytes: nil,
                width: nil,
                height: nil,
                placeholder: nil,
                alt: alt
            )
        )
    }
}
