import Foundation
import Testing
import TrUAPIHost

struct ProductIdTests {
    @Test
    func equalIdsShareNativeDictionaryEntries() throws {
        let original = try ProductId("peopl.dot")
        let canonical = try ProductId("peopl.dot")

        #expect([original: "grant"][canonical] == "grant")
    }

    @Test
    func executableIdentitySurvivesSerialization() throws {
        let original = try ProductId("app.peopl.paseo")
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
}
