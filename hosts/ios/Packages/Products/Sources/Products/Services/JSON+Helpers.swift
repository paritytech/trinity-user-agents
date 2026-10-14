import Foundation
import SubstrateSdk

public enum JSONCodingError: Error {
    case stringConversionFailed
}

extension JSON {
    func encodedString() throws -> String {
        let data = try JSONEncoder().encode(self)
        guard let string = String(data: data, encoding: .utf8) else {
            throw JSONCodingError.stringConversionFailed
        }

        return string
    }
}
