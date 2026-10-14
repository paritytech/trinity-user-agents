import Foundation
import Testing
@testable import FoundationExt

@Suite("JWTParser")
struct JWTParserTests {
    // MARK: - Well-formed tokens

    @Test("Parses a well-formed JWT with integer exp")
    func parsesIntegerExp() throws {
        let token = makeJWT(payload: #"{"sub":"a","exp":1700000000}"#)
        let payload = try JWTParser.parse(token)

        #expect(payload.exp == Date(timeIntervalSince1970: 1_700_000_000))
    }

    @Test("Parses a well-formed JWT with fractional exp")
    func parsesDoubleExp() throws {
        let token = makeJWT(payload: #"{"exp":1700000000.5}"#)
        let payload = try JWTParser.parse(token)

        #expect(payload.exp == Date(timeIntervalSince1970: 1_700_000_000.5))
    }

    @Test("Parses a large exp value")
    func parsesLargeExp() throws {
        let token = makeJWT(payload: #"{"exp":9999999999}"#)
        let payload = try JWTParser.parse(token)

        #expect(payload.exp == Date(timeIntervalSince1970: 9_999_999_999))
    }

    @Test("Parses iat and nbf claims")
    func parsesIatAndNbf() throws {
        let token = makeJWT(payload: #"{"exp":2000,"iat":1000,"nbf":1500}"#)
        let payload = try JWTParser.parse(token)

        #expect(payload.iat == Date(timeIntervalSince1970: 1_000))
        #expect(payload.nbf == Date(timeIntervalSince1970: 1_500))
    }

    @Test("Preserves extra claims in raw dictionary")
    func preservesExtraClaims() throws {
        let token = makeJWT(payload: #"{"exp":1,"sub":"user-123","role":"admin"}"#)
        let payload = try JWTParser.parse(token)

        #expect(payload.claims["sub"] as? String == "user-123")
        #expect(payload.claims["role"] as? String == "admin")
    }

    @Test("Missing exp claim returns nil without throwing")
    func missingExpReturnsNil() throws {
        let token = makeJWT(payload: #"{"sub":"a"}"#)
        let payload = try JWTParser.parse(token)

        #expect(payload.exp == nil)
    }

    // MARK: - Base64URL handling

    @Test("Handles base64url characters - and _")
    func handlesBase64URLCharacters() throws {
        // Payload that base64-encodes to contain both '+' and '/' so base64url
        // encoding replaces them with '-' and '_'.
        let payloadBytes = Data([0xFB, 0xFF, 0xBF])
        let standard = payloadBytes.base64EncodedString()
        #expect(standard.contains("+") || standard.contains("/"))

        let urlSafe = standard
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")

        // Not valid JSON, so we expect invalidJSON (but base64 decoding must succeed)
        let token = "header.\(urlSafe).sig"
        #expect(throws: JWTParsingError.invalidJSON) {
            _ = try JWTParser.parse(token)
        }
    }

    @Test("Handles payloads missing base64 padding")
    func handlesMissingPadding() throws {
        // Generate payloads of lengths that produce 0, 1, 2, 3 leftover chars
        // after base64 (i.e. 1, 2, or 3 padding chars required).
        for length in 1 ... 4 {
            let json = #"{"exp":\#(String(repeating: "1", count: length))}"#
            let token = makeJWT(payload: json, stripPadding: true)
            let payload = try JWTParser.parse(token)
            #expect(payload.exp != nil)
        }
    }

    // MARK: - Malformed input

    @Test("Throws invalidFormat for wrong segment count", arguments: [
        "",
        "only-one",
        "only.two",
        "a.b.c.d"
    ])
    func throwsOnWrongSegmentCount(token: String) {
        #expect(throws: JWTParsingError.invalidFormat) {
            _ = try JWTParser.parse(token)
        }
    }

    @Test("Throws invalidBase64 for non-base64 payload")
    func throwsOnInvalidBase64() {
        let token = "header.!!!not-base64!!!.sig"
        #expect(throws: JWTParsingError.invalidBase64) {
            _ = try JWTParser.parse(token)
        }
    }

    @Test("Throws invalidJSON for non-JSON payload")
    func throwsOnInvalidJSON() {
        // "hello world" base64url-encoded — valid base64, invalid JSON
        let payload = Data("hello world".utf8).base64EncodedString()
            .replacingOccurrences(of: "=", with: "")
        let token = "header.\(payload).sig"

        #expect(throws: JWTParsingError.invalidJSON) {
            _ = try JWTParser.parse(token)
        }
    }

    @Test("Throws invalidJSON when payload is a JSON array, not object")
    func throwsOnNonObjectJSON() {
        let payload = Data("[1,2,3]".utf8).base64EncodedString()
            .replacingOccurrences(of: "=", with: "")
        let token = "header.\(payload).sig"

        #expect(throws: JWTParsingError.invalidJSON) {
            _ = try JWTParser.parse(token)
        }
    }

    @Test("Non-numeric exp claim is ignored")
    func nonNumericExpIgnored() throws {
        let token = makeJWT(payload: #"{"exp":"not-a-number"}"#)
        let payload = try JWTParser.parse(token)

        #expect(payload.exp == nil)
    }
}

// MARK: - Helpers

private func makeJWT(payload: String, stripPadding: Bool = false) -> String {
    let header = Data(#"{"alg":"HS256"}"#.utf8).base64EncodedString()
    var payloadEncoded = Data(payload.utf8).base64EncodedString()
    if stripPadding {
        payloadEncoded = payloadEncoded.replacingOccurrences(of: "=", with: "")
    }
    return "\(header).\(payloadEncoded).sig"
}
