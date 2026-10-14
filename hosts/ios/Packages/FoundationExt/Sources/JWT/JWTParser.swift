import Foundation

// MARK: - JWT Parsing

//
// A JSON Web Token (JWT, RFC 7519) is a compact, URL-safe representation of
// a set of claims. It has the form:
//
//     BASE64URL(header) "." BASE64URL(payload) "." BASE64URL(signature)
//
// Each segment is encoded with base64url (RFC 4648 §5): standard base64 with
// `+`/`/` replaced by `-`/`_`, and trailing `=` padding stripped.
//
// This parser is intentionally minimal — it only decodes the payload to
// extract claims. It does NOT:
//   - validate the signature (the signing key is server-side)
//   - inspect the header (`alg`, `typ`, `kid`)
//   - enforce `exp` / `nbf` — callers decide the policy
//
// Registered claims handled explicitly (RFC 7519 §4.1):
//   - `exp` (expiration time) — NumericDate, seconds since the Unix epoch
//   - `iat` (issued at)       — NumericDate
//   - `nbf` (not before)      — NumericDate
// All remaining claims (registered, public, or private) are surfaced via
// `JWTPayload.claims`.

public enum JWTParsingError: Error, Equatable {
    /// Token does not have exactly three `.`-separated segments.
    case invalidFormat
    /// The payload segment is not valid base64url.
    case invalidBase64
    /// The decoded payload is not a JSON object.
    case invalidJSON
}

/// Decoded JWT claims (RFC 7519 §4).
///
/// Only the three time-valued registered claims are typed as `Date`;
/// everything else is accessible via `claims`.
public struct JWTPayload {
    /// Expiration time (`exp`), RFC 7519 §4.1.4.
    public let exp: Date?
    /// Issued at (`iat`), RFC 7519 §4.1.6.
    public let iat: Date?
    /// Not before (`nbf`), RFC 7519 §4.1.5.
    public let nbf: Date?
    /// Raw claim set, including the three typed claims above.
    public let claims: [String: Any]

    public init(claims: [String: Any]) {
        self.claims = claims
        exp = Self.date(forClaim: "exp", in: claims)
        iat = Self.date(forClaim: "iat", in: claims)
        nbf = Self.date(forClaim: "nbf", in: claims)
    }

    private static func date(forClaim key: String, in claims: [String: Any]) -> Date? {
        guard let value = claims[key] else { return nil }
        if let number = value as? TimeInterval {
            return Date(timeIntervalSince1970: number)
        }
        if let number = value as? NSNumber {
            return Date(timeIntervalSince1970: number.doubleValue)
        }
        return nil
    }
}

/// Parses JWTs into `JWTPayload` without verifying the signature.
public enum JWTParser {
    public static func parse(_ token: String) throws -> JWTPayload {
        let segments = token.split(separator: ".", omittingEmptySubsequences: false)
        guard segments.count == 3 else {
            throw JWTParsingError.invalidFormat
        }

        guard let data = decodeBase64URL(String(segments[1])) else {
            throw JWTParsingError.invalidBase64
        }

        guard let claims = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw JWTParsingError.invalidJSON
        }

        return JWTPayload(claims: claims)
    }

    private static func decodeBase64URL(_ input: String) -> Data? {
        var base64 = input
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")

        let remainder = base64.count % 4
        if remainder > 0 {
            base64.append(String(repeating: "=", count: 4 - remainder))
        }

        return Data(base64Encoded: base64)
    }
}
