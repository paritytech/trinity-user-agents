import Foundation
import MessageExchangeKit

extension MessageExchangeSignInHostCoordinator {
    actor State {
        private var exchangeService: AnyMessageExchangeService<OpaquePolkadotHostRemoteMessage>?
        private var hostsByAccountId = [Data: PolkadotSignInHost]()
        private var hostSubscriptionTask: Task<Void, Never>?

        func host(forAccountId accountId: Data) async -> PolkadotSignInHost? {
            hostsByAccountId[accountId]
        }

        func setExchangeService(_ value: AnyMessageExchangeService<OpaquePolkadotHostRemoteMessage>?) async {
            exchangeService = value
        }

        func setHostsByAccountId(_ value: [Data: PolkadotSignInHost]) async {
            hostsByAccountId = value
        }

        func setHostSubscriptionTask(_ value: Task<Void, Never>?) async {
            hostSubscriptionTask = value
        }

        func updateSessionRequests(_ requests: Set<MessageExchange.SessionRequest>) async {
            exchangeService?.updateSessions(requests)
        }

        func reset() async {
            exchangeService?.updateSessions([])
            hostsByAccountId = [:]
            hostSubscriptionTask?.cancel()
            hostSubscriptionTask = nil
        }
    }
}
