import Foundation

/// Long-term storage where pool entries end up after retention expires (chat RFC 0001).
public protocol LongTermRemoteStoring {
    func downloadData(by fileHash: FileHash) async throws -> Data?
}
