import Foundation
import Testing
@testable import FoundationExt

@Suite("Data+PrintableText")
struct PrintableTextTests {
    @Test("Decodes printable UTF-8 text")
    func decodesPrintableText() {
        #expect(Data("hello world".utf8).printableUTF8String == "hello world")
        #expect(Data("multi\nline\ttext".utf8).printableUTF8String == "multi\nline\ttext")
        #expect(Data("привет 🌍".utf8).printableUTF8String == "привет 🌍")
    }

    @Test("Rejects non-text bytes")
    func rejectsNonTextBytes() {
        #expect(Data([0x00, 0x01, 0x02]).printableUTF8String == nil)
        #expect(Data([0xFF, 0xFE, 0x85]).printableUTF8String == nil)
        #expect(Data("text\u{07}bell".utf8).printableUTF8String == nil)
    }

    @Test("Rejects empty data")
    func rejectsEmptyData() {
        #expect(Data().printableUTF8String == nil)
    }
}
