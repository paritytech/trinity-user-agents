import Foundation
import Testing
@testable import polkadot_app

@Suite("BearerTokenRequestModifier")
struct BearerTokenRequestModifierTests {
    @Test("Sets Authorization header with Bearer prefix")
    func setsAuthorizationHeader() throws {
        let modifier = BearerTokenRequestModifier(token: "test-token")
        var request = URLRequest(url: URL(string: "https://example.com")!)

        try modifier.visit(request: &request)

        #expect(request.value(forHTTPHeaderField: "Authorization") == "Bearer test-token")
    }

    @Test("Preserves existing headers")
    func preservesExistingHeaders() throws {
        let modifier = BearerTokenRequestModifier(token: "test-token")
        var request = URLRequest(url: URL(string: "https://example.com")!)
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        try modifier.visit(request: &request)

        #expect(request.value(forHTTPHeaderField: "Content-Type") == "application/json")
        #expect(request.value(forHTTPHeaderField: "Authorization") == "Bearer test-token")
    }

    @Test("Handles JWT-formatted token")
    func handlesJWTToken() throws {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.signature"
        let modifier = BearerTokenRequestModifier(token: jwt)
        var request = URLRequest(url: URL(string: "https://example.com")!)

        try modifier.visit(request: &request)

        #expect(request.value(forHTTPHeaderField: "Authorization") == "Bearer \(jwt)")
    }
}
