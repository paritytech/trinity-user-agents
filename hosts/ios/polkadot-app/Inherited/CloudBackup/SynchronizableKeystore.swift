import Foundation
import Security

protocol SynchronizableKeychainProtocol {
    func addKey(_ key: Data, with identifier: String) throws
    func updateKey(_ key: Data, with identifier: String) throws
    func fetchKey(for identifier: String) throws -> Data
    func checkKey(for identifier: String) throws -> Bool
    func deleteKey(for identifier: String) throws

    var isAvailable: Bool { get }
    var didChangeAvailabilityNotification: Notification.Name { get }
}

enum SynchronizableKeychainError: Error {
    case invalidIdentifierFormat
    case noKeyFound
    case duplicatedItem
    case unexpectedFail
}

class SynchronizableKeychain: SynchronizableKeychainProtocol {
    let fileManager: FileManager

    init(fileManager: FileManager = .default) {
        self.fileManager = fileManager
    }

    var isAvailable: Bool {
        fileManager.ubiquityIdentityToken != nil
    }

    var didChangeAvailabilityNotification: Notification.Name {
        .NSUbiquityIdentityDidChange
    }

    func addKey(_ key: Data, with identifier: String) throws {
        guard let applicationTag = identifier.data(using: String.Encoding.utf8) else {
            throw SynchronizableKeychainError.invalidIdentifierFormat
        }

        let attributes: [String: Any] = [
            kSecClass as String: kSecClassKey,
            kSecAttrApplicationTag as String: applicationTag,
            kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlocked,
            kSecAttrSynchronizable as String: kCFBooleanTrue as Any,
            kSecValueData as String: key
        ]

        let status = SecItemAdd(attributes as CFDictionary, nil)

        let optionalError = keystoreError(for: status)
        guard optionalError == nil else { throw optionalError! }
    }

    func updateKey(_ key: Data, with identifier: String) throws {
        guard let applicationTag = identifier.data(using: String.Encoding.utf8) else {
            throw SynchronizableKeychainError.invalidIdentifierFormat
        }

        let query: [String: Any] = [
            kSecClass as String: kSecClassKey,
            kSecAttrSynchronizable as String: kCFBooleanTrue as Any,
            kSecAttrApplicationTag as String: applicationTag
        ]

        let attributesToUpdate: [String: Any] = [
            kSecValueData as String: key
        ]

        let status = SecItemUpdate(query as CFDictionary, attributesToUpdate as CFDictionary)

        let optionalError = keystoreError(for: status)
        guard optionalError == nil else { throw optionalError! }
    }

    func fetchKey(for identifier: String) throws -> Data {
        guard let applicationTag = identifier.data(using: String.Encoding.utf8) else {
            throw SynchronizableKeychainError.invalidIdentifierFormat
        }

        let query: [String: Any] = [
            kSecClass as String: kSecClassKey,
            kSecAttrSynchronizable as String: kCFBooleanTrue as Any,
            kSecAttrApplicationTag as String: applicationTag,
            kSecReturnData as String: kCFBooleanTrue as Any
        ]

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)

        let optionalError = keystoreError(for: status)
        guard optionalError == nil else { throw optionalError! }

        guard let key = item as? Data else { throw SynchronizableKeychainError.unexpectedFail }

        return key
    }

    func checkKey(for identifier: String) throws -> Bool {
        guard let applicationTag = identifier.data(using: String.Encoding.utf8) else {
            throw SynchronizableKeychainError.invalidIdentifierFormat
        }

        let query: [String: Any] = [
            kSecClass as String: kSecClassKey,
            kSecAttrSynchronizable as String: kCFBooleanTrue as Any,
            kSecAttrApplicationTag as String: applicationTag,
            kSecReturnData as String: kCFBooleanFalse as Any
        ]

        let status = SecItemCopyMatching(query as CFDictionary, nil)

        let optionalError = keystoreError(for: status)

        if optionalError == SynchronizableKeychainError.noKeyFound { return false }

        guard optionalError == nil else { throw optionalError! }

        return true
    }

    func deleteKey(for identifier: String) throws {
        guard let applicationTag = identifier.data(using: String.Encoding.utf8) else {
            throw SynchronizableKeychainError.invalidIdentifierFormat
        }

        let query: [String: Any] = [
            kSecClass as String: kSecClassKey,
            kSecAttrSynchronizable as String: kCFBooleanTrue as Any,
            kSecAttrApplicationTag as String: applicationTag
        ]

        let status = SecItemDelete(query as CFDictionary)

        let optionalError = keystoreError(for: status)
        guard optionalError == nil else { throw optionalError! }
    }

    private func keystoreError(for status: OSStatus) -> SynchronizableKeychainError? {
        guard status != errSecDuplicateItem else { return SynchronizableKeychainError.duplicatedItem }
        guard status != errSecItemNotFound else { return SynchronizableKeychainError.noKeyFound }
        guard status == errSecSuccess else { return SynchronizableKeychainError.unexpectedFail }

        return nil
    }
}
