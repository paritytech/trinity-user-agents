import SubstrateSdk
@testable import polkadot_app

final class SpyDirectMessageSender: ChatDirectMessageSending {
    private(set) var sent: [(message: Chat.RemoteMessage, accountId: AccountId)] = []

    func sendDirectly(_ remoteMessage: Chat.RemoteMessage, to peerAccountId: AccountId) {
        sent.append((remoteMessage, peerAccountId))
    }
}
