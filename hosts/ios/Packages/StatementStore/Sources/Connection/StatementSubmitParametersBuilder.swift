import Foundation
import SubstrateSdk
import BigInt
import SDKLogger

public protocol StatementSubmitParametersBuilding {
    func build() throws -> StatementSubmitParameters
}

/// Builds a statement for a signer known up front; see `UnsignedStatement` to sign at submission time.
public final class StatementSubmitParametersBuilder: StatementSubmitParametersBuilding {
    private let signer: StatementStoreSigning
    private let logger: SDKLoggerProtocol?
    private let statement = UnsignedStatement()

    public init(
        signer: StatementStoreSigning,
        logger: SDKLoggerProtocol?
    ) {
        self.signer = signer
        self.logger = logger
    }

    @discardableResult
    public func addChannel(_ value: StatementFixedFieldConvertible) -> Self {
        statement.addChannel(value)
        return self
    }

    @discardableResult
    public func addTopic1(_ value: StatementFixedFieldConvertible) -> Self {
        statement.addTopic1(value)
        return self
    }

    @discardableResult
    public func addTopic2(_ value: StatementFixedFieldConvertible) -> Self {
        statement.addTopic2(value)
        return self
    }

    @discardableResult
    public func addTopic3(_ value: StatementFixedFieldConvertible) -> Self {
        statement.addTopic3(value)
        return self
    }

    @discardableResult
    public func addTopic4(_ value: StatementFixedFieldConvertible) -> Self {
        statement.addTopic4(value)
        return self
    }

    @discardableResult
    public func addScaleEncodedPayload(_ value: Data) -> Self {
        statement.addScaleEncodedPayload(value)
        return self
    }

    @discardableResult
    public func addExpiry(_ value: UInt64) -> Self {
        statement.addExpiry(value)
        return self
    }

    public func build() throws -> StatementSubmitParameters {
        try statement.signed(by: signer).build()
    }
}
