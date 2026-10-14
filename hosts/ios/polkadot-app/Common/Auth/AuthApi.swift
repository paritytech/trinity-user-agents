import Foundation
import UniqueDevice

enum AuthApi {
    case challenge
    case attestation
    case token
    case refreshToken
}

extension AuthApi: AppAttestURLConvertible {
    var url: URL {
        let components = URLComponents(string: urlString)
        guard let url = components?.url(relativeTo: AppConfig.Backend.baseUrl) else {
            assertionFailure()
            return AppConfig.Backend.baseUrl
        }
        return url
    }

    private var urlString: String {
        switch self {
        case .challenge:
            "api/v1/auth/challenges"
        case .attestation:
            "api/v1/auth/app-attest/attestations"
        case .token:
            "api/v1/auth/token"
        case .refreshToken:
            "api/v1/auth/token/refresh"
        }
    }
}

extension AppAttestConfiguration {
    static let appAttest = AppAttestConfiguration(
        identifier: "appattest.local.settings",
        challengeUrl: AuthApi.challenge,
        attestationUrl: AuthApi.attestation
    )
}
