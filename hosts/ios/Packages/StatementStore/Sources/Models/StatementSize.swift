import Foundation

public enum StatementSize {
    /// Bytes a signed statement with an expiry, a channel and `topicCount` topics adds around its
    /// scale-encoded payload.
    public static func overhead(topicCount: Int) -> Int {
        fieldCountPrefixSize
            + fieldIndexSize + proofSize
            + fieldIndexSize + expirySize
            + fieldIndexSize + StatementFieldConstants.fixedFieldSize
            + topicCount * (fieldIndexSize + StatementFieldConstants.fixedFieldSize)
            + fieldIndexSize
    }
}

private extension StatementSize {
    static let fieldCountPrefixSize = 1
    static let fieldIndexSize = 1
    static let expirySize = 8
    static let proofSize = fieldIndexSize + StatementProof.signatureSize + StatementProof.signerSize
}
