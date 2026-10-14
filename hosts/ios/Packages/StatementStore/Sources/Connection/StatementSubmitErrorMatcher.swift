import Foundation
import SubstrateSdk

public protocol StatementSubmitErrorMatching {
    func match(error: Error) -> Bool
}

public final class StatementSubmitOneOfErrorMatcher: StatementSubmitErrorMatching {
    let matchers: [StatementSubmitErrorMatching]

    public init(matchers: [StatementSubmitErrorMatching]) {
        self.matchers = matchers
    }

    public func match(error: Error) -> Bool {
        for matcher in matchers {
            if matcher.match(error: error) {
                return true
            }
        }

        return false
    }
}

public final class StatementSubmitTimeoutMatcher: StatementSubmitErrorMatching {
    public init() {}

    public func match(error: Error) -> Bool {
        if let rpcOperationError = error as? JSONRPCOperationError {
            return rpcOperationError == .timeout
        }
        return false
    }
}

public class StatementSubmitErrorClosureMatcher: StatementSubmitErrorMatching {
    let closure: (StatementSubmitError) -> Bool

    public init(closure: @escaping (StatementSubmitError) -> Bool) {
        self.closure = closure
    }

    public func match(error: Error) -> Bool {
        guard let rpcError = error as? StatementSubmitError else {
            return false
        }

        return closure(rpcError)
    }
}

public class StatementRealSubmitErrorMatcher: StatementSubmitErrorClosureMatcher {
    public init(matchingError: StatementSubmitError) {
        super.init(closure: { submitError in
            matchingError == submitError
        })
    }

    public static func channelPriorityTooLow() -> StatementRealSubmitErrorMatcher {
        StatementRealSubmitErrorMatcher(matchingError: .rejected(.channelPriorityTooLow))
    }

    public static func noAllowance() -> StatementRealSubmitErrorMatcher {
        StatementRealSubmitErrorMatcher(matchingError: .rejected(.noAllowance))
    }
}

public enum StatementSubmitErrorMatcher {
    public static func retryWhenTimeoutOrNoAllowance() -> StatementSubmitOneOfErrorMatcher {
        StatementSubmitOneOfErrorMatcher(
            matchers: [
                StatementRealSubmitErrorMatcher.noAllowance(),
                StatementSubmitTimeoutMatcher()
            ]
        )
    }
}
