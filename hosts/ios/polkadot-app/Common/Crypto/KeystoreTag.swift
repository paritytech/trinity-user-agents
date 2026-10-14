import Foundation
import SubstrateSdk
import KeyDerivation

extension KeystoreTag {
    static func referralTicketTag(
        _ accountId: AccountId
    ) -> String {
        [
            domain,
            accountId.toHex(),
            "referralTicket"
        ].joined(with: .colon)
    }

    static func receivedRefferalTag() -> String {
        [
            domain,
            "receivedReferral"
        ].joined(with: .colon)
    }

    static func deviceEncryptionKeyTag(for encryptId: String) -> String {
        [
            domain,
            encryptId,
            "deviceEncryptionKey"
        ].joined(with: .colon)
    }

    static func backendClientTag(for sessionId: String) -> String {
        [
            domain,
            sessionId,
            "backend.client"
        ].joined(with: .colon)
    }

    static func jwtTokenTag(for sessionId: String) -> String {
        [
            domain,
            sessionId,
            "jwt.token"
        ].joined(with: .colon)
    }

    static func jwtRefreshTokenTag(for sessionId: String) -> String {
        [
            domain,
            sessionId,
            "jwt.refresh.token"
        ].joined(with: .colon)
    }

    static func coinageCoinIndexTag(forInstallation installationId: String) -> String {
        [
            domain,
            installationId,
            "coinage.coin.index"
        ].joined(with: .colon)
    }

    static func coinageVoucherIndexTag(forInstallation installationId: String) -> String {
        [
            domain,
            installationId,
            "coinage.voucher.index"
        ].joined(with: .colon)
    }
}
