import Foundation

/// A three-valued chain read.
///
/// `failedRead` is not `absent`: a transport error, an unknown block, a key missing from a batched
/// response, or an undecodable value all mean "no verdict". Every predicate built on these results must
/// be false under `failedRead` so a failed read can never produce a terminal status.
public enum ReadResult<Value: Sendable>: Sendable {
    case present(Value)
    case absent
    case failedRead
}

public extension ReadResult {
    var isPresent: Bool {
        if case .present = self { return true }
        return false
    }

    var isAbsent: Bool {
        if case .absent = self { return true }
        return false
    }

    var isFailedRead: Bool {
        if case .failedRead = self { return true }
        return false
    }

    var value: Value? {
        if case let .present(value) = self { return value }
        return nil
    }

    func map<Other: Sendable>(_ transform: (Value) -> Other) -> ReadResult<Other> {
        switch self {
        case let .present(value): .present(transform(value))
        case .absent: .absent
        case .failedRead: .failedRead
        }
    }
}
