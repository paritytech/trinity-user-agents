import Foundation
import Keystore_iOS
import SubstrateSdk

protocol ChatDiscoverySettingsStoring: AnyObject {
    func fetchLastChatRequestDay(for accountId: AccountId) -> ChatRequest.Day?
    func saveLastChatRequestDay(_ lastChatRequestDay: ChatRequest.Day?, for accountId: AccountId)
}

extension SettingsManager: ChatDiscoverySettingsStoring {
    private func chatRequestDayKey(for accountId: AccountId) -> String {
        ["lastChatRequestDay", accountId.toHex()].joined(with: .colon)
    }

    func fetchLastChatRequestDay(for accountId: AccountId) -> ChatRequest.Day? {
        let key = chatRequestDayKey(for: accountId)

        return integer(for: key).map { ChatRequest.Day($0) }
    }

    func saveLastChatRequestDay(_ lastChatRequestDay: ChatRequest.Day?, for accountId: AccountId) {
        let key = chatRequestDayKey(for: accountId)

        if let lastChatRequestDay {
            set(value: Int(lastChatRequestDay), for: key)
        } else {
            removeValue(for: key)
        }
    }
}
