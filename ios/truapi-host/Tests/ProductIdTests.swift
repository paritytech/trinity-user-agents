import Foundation
import Testing
import TrUAPIHost

struct ProductIdTests {
    @Test
    func normalizedIdsShareNativeDictionaryEntries() throws {
        let original = try ProductId(" Peopl.DOT ")
        let canonical = try ProductId("peopl.dot")

        #expect([original: "grant"][canonical] == "grant")
        #expect(original.rawValue == "peopl.dot")
    }

    @Test
    func executableIdentitySurvivesSerialization() throws {
        let original = try ProductId(" APP.Peopl.PASEO ")
        let encoded = try JSONEncoder().encode(original)

        #expect(encoded == Data("\"app.peopl.paseo\"".utf8))
        #expect(try JSONDecoder().decode(ProductId.self, from: encoded) == original)
    }

    @Test
    func ffiRoundTripPreservesNativeValue() throws {
        let original = try ProductId("worker.peopl.dot")
        let lowered = FfiConverterTypeProductId_lower(original)

        #expect(try FfiConverterTypeProductId_lift(lowered) == original)
    }

    @Test(arguments: ["", "peopl", "peopl.com", "localhost.dot", "my_app.dot", "tést.dot"])
    func invalidInputCannotBecomeAnId(value: String) {
        #expect(throws: ProductIdError.self) {
            try ProductId(value)
        }
        #expect(throws: ProductIdError.self) {
            try JSONDecoder().decode(ProductId.self, from: JSONEncoder().encode(value))
        }
    }
}
