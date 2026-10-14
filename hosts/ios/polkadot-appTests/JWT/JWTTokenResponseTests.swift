import Foundation
import Testing
@testable import polkadot_app

@Suite("JWTTokenResponse")
struct JWTTokenResponseTests {
    @Test("Decodes valid response with token and refreshToken")
    func decodesValid() throws {
        let json = """
        {"token": "eyJhbGciOiJIUzI1NiJ9.test.signature", "refreshToken": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"}
        """.data(using: .utf8)!

        let response = try JSONDecoder().decode(JWTTokenResponse.self, from: json)
        #expect(response.token == "eyJhbGciOiJIUzI1NiJ9.test.signature")
        #expect(response.refreshToken == "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2")
    }

    @Test("Fails on missing token field")
    func failsOnMissingToken() {
        let json = """
        {"refreshToken": "abc123"}
        """.data(using: .utf8)!

        #expect(throws: DecodingError.self) {
            try JSONDecoder().decode(JWTTokenResponse.self, from: json)
        }
    }

    @Test("Fails on missing refreshToken field")
    func failsOnMissingRefreshToken() {
        let json = """
        {"token": "eyJhbGciOiJIUzI1NiJ9.test.signature"}
        """.data(using: .utf8)!

        #expect(throws: DecodingError.self) {
            try JSONDecoder().decode(JWTTokenResponse.self, from: json)
        }
    }

    @Test("Fails on empty JSON object")
    func failsOnEmptyObject() {
        let json = "{}".data(using: .utf8)!

        #expect(throws: DecodingError.self) {
            try JSONDecoder().decode(JWTTokenResponse.self, from: json)
        }
    }
}

@Suite("RefreshTokenRequest")
struct RefreshTokenRequestTests {
    @Test("Encodes correctly")
    func encodesCorrectly() throws {
        let request = RefreshTokenRequest(refreshToken: "a1b2c3d4")
        let data = try JSONEncoder().encode(request)
        let json = try JSONDecoder().decode([String: String].self, from: data)

        #expect(json["refreshToken"] == "a1b2c3d4")
    }
}
