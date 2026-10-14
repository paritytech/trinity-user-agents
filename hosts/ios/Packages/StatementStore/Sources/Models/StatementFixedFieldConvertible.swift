import Foundation

public protocol StatementFixedFieldConvertible {
    func fixedStatementFieldData() throws -> Data
}

public enum StatementFixedFieldConvertibleError: Error {
    case conversionFailed(String)
}

extension Data: StatementFixedFieldConvertible {
    public func fixedStatementFieldData() throws -> Data {
        if count != StatementFieldConstants.fixedFieldSize {
            try blake2b32()
        } else {
            self
        }
    }
}

extension String: StatementFixedFieldConvertible {
    public func fixedStatementFieldData() throws -> Data {
        try Data(utf8).fixedStatementFieldData()
    }
}
