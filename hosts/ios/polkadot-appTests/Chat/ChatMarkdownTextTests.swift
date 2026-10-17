import Foundation
import PolkadotUI
import Testing

/// The Markdown subset of product rich text, 02-DESIGN §6.1.
struct ChatMarkdownTextTests {
    @Test func boldItalicAndInlineCodeKeepTheirStyle() throws {
        let text = render("a **bold** and *slanted* `code`")

        #expect(characters(text) == "a bold and slanted code")
        #expect(try intent(of: "bold", in: text)?.contains(.stronglyEmphasized) == true)
        #expect(try intent(of: "slanted", in: text)?.contains(.emphasized) == true)
        #expect(try intent(of: "code", in: text)?.contains(.code) == true)
    }

    @Test func aHeadingIsABoldParagraph() throws {
        let text = render("## Results\nbody")

        #expect(characters(text) == "Results\n\nbody")
        #expect(try intent(of: "Results", in: text)?.contains(.stronglyEmphasized) == true)
    }

    @Test func paragraphsAreSeparatedByABlankLine() {
        #expect(characters(render("one\n\ntwo")) == "one\n\ntwo")
    }

    /// A chat message keeps the line breaks that its writer typed. CommonMark lets a soft line break show as a
    /// line break.
    @Test func aLineEndingInsideAParagraphIsALineBreak() throws {
        let text = render("**first** line\nsecond *line*")

        #expect(characters(text) == "first line\nsecond line")
        #expect(try intent(of: "first", in: text)?.contains(.stronglyEmphasized) == true)
    }

    @Test func listsShowTheirMarkers() {
        let text = render("- red\n- blue\n  - navy\n\n1. first\n2. second")

        #expect(characters(text) == "• red\n• blue\n  • navy\n\n1. first\n2. second")
    }

    @Test func aBlockQuoteIsMarked() {
        #expect(characters(render("> said")) == "▎ said")
    }

    @Test func aFencedCodeBlockIsMonospacedWithoutItsTrailingLineBreak() throws {
        let text = render("```swift\nlet x = 1\n```")

        #expect(characters(text) == "let x = 1")
        #expect(try intent(of: "let x = 1", in: text) == .code)
    }

    @Test func anHttpsLinkStaysALink() throws {
        let text = render("see [docs](https://example.com/docs)")
        let range = try #require(text.range(of: "docs"))

        #expect(text[range].link == URL(string: "https://example.com/docs"))
    }

    @Test func anHttpLinkStaysALink() throws {
        let text = render("see [docs](http://example.com/docs)")
        let range = try #require(text.range(of: "docs"))

        #expect(text[range].link == URL(string: "http://example.com/docs"))
    }

    /// A person writes a bare address in a Markdown text too. A bare domain opens as `https`; code stays text.
    @Test func aBareAddressIsALinkOutOfCode() throws {
        let text = render("**read** www.example.net and `example.org`")
        let domain = try #require(text.range(of: "www.example.net"))
        let code = try #require(text.range(of: "example.org"))

        #expect(text[domain].link == URL(string: "https://www.example.net"))
        #expect(text[code].link == nil)
    }

    @Test(arguments: ["[x](javascript:alert(1))", "[x](tel:123)", "[x](mailto:a@example.com)"])
    func anyOtherLinkIsOnlyText(markdown: String) throws {
        let text = render(markdown)
        let range = try #require(text.range(of: "x"))

        #expect(characters(text) == "x")
        #expect(text[range].link == nil)
    }

    @Test func anImageShowsItsAlternativeTextOnly() throws {
        let text = render("![a cat](https://example.com/cat.png)")
        let range = try #require(text.range(of: "a cat"))

        #expect(characters(text) == "a cat")
        #expect(text[range].imageURL == nil)
    }

    @Test func htmlShowsItsTextWithoutTags() {
        #expect(characters(render("a <b>bold</b> word")) == "a bold word")
        #expect(characters(render("<div>\nblock <i>html</i>\n</div>\n\nafter")) == "block html\n\nafter")
    }

    @Test func aTableShowsItsCellsAsText() {
        #expect(characters(render("| a | b |\n|---|---|\n| 1 | 2 |")) == "a | b\n1 | 2")
    }
}

private extension ChatMarkdownTextTests {
    func render(_ markdown: String) -> AttributedString {
        ChatMarkdownText.attributedString(from: markdown)
    }

    func characters(_ text: AttributedString) -> String {
        String(text.characters)
    }

    func intent(of substring: String, in text: AttributedString) throws -> InlinePresentationIntent? {
        let range = try #require(text.range(of: substring))
        return text[range].inlinePresentationIntent
    }
}
