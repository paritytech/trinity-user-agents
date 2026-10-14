import Foundation

struct JWTTokenResponse: Decodable {
    let token: String
    let refreshToken: String
}

struct RefreshTokenRequest: Encodable {
    let refreshToken: String
}
