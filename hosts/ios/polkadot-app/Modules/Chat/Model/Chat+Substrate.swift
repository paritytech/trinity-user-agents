import Foundation
import SubstrateSdk
import SubstrateSdkExt
import Individuality

extension Chat.RemoteContact {
    enum RemoteError: Error {
        case invalidUsername
        case missingChatKey
    }

    init(accountName: DotnsGatewayPallet.AccountNameWithAccountId) throws {
        let chatKeyData = try accountName.record.chatKey.mapOrThrow(RemoteError.missingChatKey)

        let chatPublicKey = try Chat.OnChainEncryptionIdentifier
            .fromScaleEncoded(chatKeyData)
            .localPublicKey

        let usernameData = try accountName.record.username.mapOrThrow(RemoteError.invalidUsername)

        let username = try String(data: usernameData, encoding: .utf8).mapOrThrow(
            RemoteError.invalidUsername
        )

        self.init(
            accountId: accountName.accountId,
            username: username,
            chatPublicKey: chatPublicKey,
            imageData: nil
        )
    }
}
