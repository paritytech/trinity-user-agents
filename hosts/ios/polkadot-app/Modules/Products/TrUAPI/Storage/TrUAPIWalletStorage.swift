import CryptoKit
import Darwin
import Foundation
import TrUAPIHost

/// Host-owned encrypted wallet snapshots, outside every product's storage.
/// Only the new main-purse and native-Chat slots use this backend; existing
/// UserDefaults slots are unchanged. One process-wide lock covers all bridges.
final class TrUAPIWalletStorage: @unchecked Sendable {
    static let shared = TrUAPIWalletStorage()
    private let lock = NSLock()

    /// Stable SCALE discriminants from truapi_platform::CoreStorageKey.
    /// The remaining encoded bytes include wallet root, genesis, and (for the
    /// device) product. They are all included in the backing filename digest.
    static func owns(_ key: Data) -> Bool {
        switch key.first {
        case 13, 14, 15, 16: true
        default: false
        }
    }

    func read(key: Data) throws -> Data? {
        try requireExclusiveCustody(key)
        lock.lock()
        defer { lock.unlock() }
        let file = try fileURL(key: key)
        do {
            return try Data(contentsOf: file)
        } catch let error as NSError
            where error.domain == NSCocoaErrorDomain && error.code == NSFileReadNoSuchFileError {
            return nil
        }
    }

    func write(key: Data, value: Data) throws {
        try requireExclusiveCustody(key)
        lock.lock()
        defer { lock.unlock() }
        let file = try fileURL(key: key)
        let directory = file.deletingLastPathComponent()
        let temporary = directory.appendingPathComponent(".pending-\(UUID().uuidString)")
        let descriptor = temporary.path.withCString {
            Darwin.open($0, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, mode_t(0o600))
        }
        guard descriptor >= 0 else { throw failure("create temporary snapshot") }
        var open = true
        defer {
            if open { Darwin.close(descriptor) }
            // Never remove the destination on failure: after rename it may be
            // the only surviving durable operation journal.
            temporary.path.withCString { _ = Darwin.unlink($0) }
        }
        try FileManager.default.setAttributes(
            [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
            ofItemAtPath: temporary.path
        )
        try value.withUnsafeBytes { bytes in
            var offset = 0
            while offset < bytes.count {
                let written = Darwin.write(descriptor, bytes.baseAddress!.advanced(by: offset), bytes.count - offset)
                if written < 0 {
                    if errno == EINTR { continue }
                    throw failure("write temporary snapshot")
                }
                guard written > 0 else { throw IOFailure(operation: "write temporary snapshot", code: EIO, ambiguous: false) }
                offset += written
            }
        }
        guard Darwin.fsync(descriptor) == 0 else { throw failure("sync temporary snapshot") }
        // fsync alone may stop at a device cache on Apple platforms.
        guard Darwin.fcntl(descriptor, F_FULLFSYNC) == 0 else { throw failure("flush temporary snapshot") }
        let closeResult = Darwin.close(descriptor)
        open = false
        guard closeResult == 0 else { throw failure("close temporary snapshot") }
        let replaced = temporary.path.withCString { source in
            file.path.withCString { destination in Darwin.rename(source, destination) }
        }
        guard replaced == 0 else { throw failure("replace wallet snapshot") }
        // A failure here is AMBIGUOUS, not proof of no write. Core must poison
        // its in-memory store and reconcile the journal after reopening.
        try syncDirectory(directory, ambiguous: true)
    }

    func clear(key: Data) throws {
        try requireExclusiveCustody(key)
        lock.lock()
        defer { lock.unlock() }
        let file = try fileURL(key: key)
        let removed = file.path.withCString { Darwin.unlink($0) }
        guard removed == 0 || errno == ENOENT else { throw failure("remove wallet snapshot") }
        try syncDirectory(file.deletingLastPathComponent(), ambiguous: true)
    }

    /// This reference wallet still spends through its independent native
    /// CoinageService. A missing Rust snapshot is not an empty purse: refuse
    /// before Core can allocate or claim inventory owned by the native ledger.
    private func requireExclusiveCustody(_ key: Data) throws {
        guard key.first != 13 else {
            throw HostRejection.Rejected(reason: "main-purse custody unavailable: native CoinageService owns this wallet")
        }
    }

    private func fileURL(key: Data) throws -> URL {
        let manager = FileManager.default
        let support = try manager.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        var directory = support.appendingPathComponent("TrUAPIWalletState", isDirectory: true)
        try manager.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [
            .posixPermissions: 0o700,
            .protectionKey: FileProtectionType.completeUntilFirstUserAuthentication
        ])
        try manager.setAttributes([
            .posixPermissions: 0o700,
            .protectionKey: FileProtectionType.completeUntilFirstUserAuthentication
        ], ofItemAtPath: directory.path)
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try directory.setResourceValues(values)
        // Persist creation before acknowledging a snapshot in this directory.
        try syncDirectory(directory, ambiguous: false)
        try syncDirectory(support, ambiguous: false)
        try syncDirectory(support.deletingLastPathComponent(), ambiguous: false)
        // SHA-256 of the ENTIRE SCALE key is stable and collision-resistant,
        // including product length framing. Unlike raw hex, it cannot exceed
        // NAME_MAX for a long but valid product identifier.
        let name = SHA256.hash(data: key).map { String(format: "%02x", $0) }.joined()
        return directory.appendingPathComponent(name + ".snapshot")
    }

    private func syncDirectory(_ directory: URL, ambiguous: Bool) throws {
        let descriptor = directory.path.withCString { Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW) }
        guard descriptor >= 0 else { throw failure("open snapshot directory", ambiguous: ambiguous) }
        defer { Darwin.close(descriptor) }
        guard Darwin.fsync(descriptor) == 0 else { throw failure("sync snapshot directory", ambiguous: ambiguous) }
    }

    private func failure(_ operation: String, ambiguous: Bool = false) -> IOFailure {
        IOFailure(operation: operation, code: errno, ambiguous: ambiguous)
    }

    private struct IOFailure: Error, CustomStringConvertible {
        let operation: String
        let code: Int32
        let ambiguous: Bool

        var description: String {
            "Wallet storage \(operation) failed (errno \(code)); \(ambiguous ? "durability unknown; reopen and reconcile before spending" : "replacement not acknowledged")"
        }
    }
}
