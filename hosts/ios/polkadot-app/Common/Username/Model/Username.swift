import Foundation

struct Username: Hashable {
    private static let separator = "."

    let value: String
    let digits: String?

    init(value: String) {
        self.value = value

        let parts = value.split(
            separator: Self.separator,
            maxSplits: 1,
            omittingEmptySubsequences: false
        )
        if parts.count == 2, !parts[1].isEmpty {
            digits = String(parts[1])
        } else {
            digits = nil
        }
    }

    init(name: String, digits: Int) {
        let paddedDigits = String(format: "%02d", digits)
        value = "\(name)\(Self.separator)\(paddedDigits)"
        self.digits = paddedDigits
    }
}

extension Username {
    enum Constants {
        static let minLength: Int = 7
        static let maxLength: Int = 32
    }
}

extension Username {
    init?(rawData: Data) {
        guard let string = String(data: rawData, encoding: .utf8) else {
            return nil
        }

        self.init(value: string)
    }

    var partialUsername: String {
        let username = value.split(
            separator: Self.separator,
            maxSplits: 1,
            omittingEmptySubsequences: false
        ).first
        return username.map { String($0) } ?? ""
    }

    var suffix: String {
        let suffix = value.split(
            separator: Self.separator,
            maxSplits: 1,
            omittingEmptySubsequences: false
        ).last
        return suffix.map { Self.separator + String($0) } ?? ""
    }

    var numericSuffix: Int {
        Int(value.split(separator: ".").last ?? "") ?? 0
    }
}

extension Username: Comparable {
    static func < (lhs: Username, rhs: Username) -> Bool {
        let leftPrefix = lhs.partialUsername.lowercased()
        let rightPrefix = rhs.partialUsername.lowercased()

        if leftPrefix != rightPrefix {
            return leftPrefix < rightPrefix
        }

        return lhs.numericSuffix < rhs.numericSuffix
    }
}
