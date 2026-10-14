import Foundation
import AsyncExtensions
import Keystore_iOS

protocol TabBarLabelsProviding {
    func stream() -> AnyAsyncSequence<Bool>
    func save(isEnabled: Bool)
}

final class TabBarLabelsStore: TabBarLabelsProviding {
    static let shared = TabBarLabelsStore()

    private let settingsManager: SettingsManagerProtocol
    private let subject: AsyncCurrentValueSubject<Bool>

    init(settingsManager: SettingsManagerProtocol = SettingsManager.shared) {
        self.settingsManager = settingsManager

        let stored = settingsManager.value(for: .tabBarLabelsEnabled)
        subject = AsyncCurrentValueSubject<Bool>(stored)
    }

    func stream() -> AnyAsyncSequence<Bool> {
        subject.eraseToAnyAsyncSequence()
    }

    func save(isEnabled: Bool) {
        settingsManager.set(value: isEnabled, for: .tabBarLabelsEnabled)
        subject.send(isEnabled)
    }
}
