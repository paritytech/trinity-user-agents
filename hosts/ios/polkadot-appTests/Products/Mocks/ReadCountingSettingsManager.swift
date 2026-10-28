import Foundation
import Keystore_iOS

/// Forwards to an in-memory store and records the keys read as strings.
final class ReadCountingSettingsManager: SettingsManagerProtocol {
    private let store = InMemorySettingsManager()
    private(set) var stringReads: [String] = []

    func set(value: Bool, for key: String) { store.set(value: value, for: key) }
    func set(value: Int, for key: String) { store.set(value: value, for: key) }
    func set(value: Double, for key: String) { store.set(value: value, for: key) }
    func set(value: String, for key: String) { store.set(value: value, for: key) }
    func set(value: Data, for key: String) { store.set(value: value, for: key) }
    func set(anyValue: Any, for key: String) { store.set(anyValue: anyValue, for: key) }
    func bool(for key: String) -> Bool? { store.bool(for: key) }
    func integer(for key: String) -> Int? { store.integer(for: key) }
    func double(for key: String) -> Double? { store.double(for: key) }
    func data(for key: String) -> Data? { store.data(for: key) }
    func anyValue(for key: String) -> Any? { store.anyValue(for: key) }
    func removeValue(for key: String) { store.removeValue(for: key) }
    func removeAll() { store.removeAll() }

    func string(for key: String) -> String? {
        stringReads.append(key)
        return store.string(for: key)
    }
}
