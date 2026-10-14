import Foundation
import Keystore_iOS
import ChainRegistry
import EventCenter

protocol SelectedCurrencyManaging {
    var selectedCurrency: Currency { get }
    func save(currency: Currency)
}

final class SelectedCurrencyManager: SelectedCurrencyManaging {
    static let shared = SelectedCurrencyManager()

    private let settingsManager: SettingsManagerProtocol
    private let eventCenter: EventCenterProtocol

    var selectedCurrency: Currency {
        guard
            let code = settingsManager.string(for: SettingsKey.selectedCurrencyCode.rawValue),
            let currency = Currency.supported.first(where: { $0.code == code })
        else {
            return .usd
        }

        return currency
    }

    init(
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        eventCenter: EventCenterProtocol = EventCenter.shared
    ) {
        self.settingsManager = settingsManager
        self.eventCenter = eventCenter
    }

    func save(currency: Currency) {
        settingsManager.set(value: currency.code, for: SettingsKey.selectedCurrencyCode.rawValue)
        eventCenter.notify(with: SelectedCurrencyChanged())
    }
}
