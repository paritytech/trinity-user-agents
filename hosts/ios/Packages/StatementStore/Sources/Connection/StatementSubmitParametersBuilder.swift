import Foundation
import SubstrateSdk
import BigInt
import SDKLogger

public protocol StatementSubmitParametersBuilding {
    func build() throws -> StatementSubmitParameters
}

public final class StatementSubmitParametersBuilder: StatementSubmitParametersBuilding {
    typealias FieldConverter = () throws -> StatementField

    private let signer: StatementStoreSigning
    private let logger: SDKLoggerProtocol?
    private var fieldsToConvert = [FieldConverter]()

    public init(
        signer: StatementStoreSigning,
        logger: SDKLoggerProtocol?
    ) {
        self.signer = signer
        self.logger = logger
    }

    @discardableResult
    public func addChannel(_ value: StatementFixedFieldConvertible) -> Self {
        addFieldToConvert {
            let data = try value.fixedStatementFieldData()
            return .channel(data)
        }
    }

    @discardableResult
    public func addTopic1(_ value: StatementFixedFieldConvertible) -> Self {
        addFieldToConvert {
            let data = try value.fixedStatementFieldData()
            return .topic1(data)
        }
    }

    @discardableResult
    public func addTopic2(_ value: StatementFixedFieldConvertible) -> Self {
        addFieldToConvert {
            let data = try value.fixedStatementFieldData()
            return .topic2(data)
        }
    }

    @discardableResult
    public func addTopic3(_ value: StatementFixedFieldConvertible) -> Self {
        addFieldToConvert {
            let data = try value.fixedStatementFieldData()
            return .topic3(data)
        }
    }

    @discardableResult
    public func addTopic4(_ value: StatementFixedFieldConvertible) -> Self {
        addFieldToConvert {
            let data = try value.fixedStatementFieldData()
            return .topic4(data)
        }
    }

    @discardableResult
    public func addScaleEncodedPayload(_ value: Data) -> Self {
        addFieldToConvert {
            .scaleEncodedPayload(value)
        }
    }

    @discardableResult
    public func addExpiry(_ value: UInt64) -> Self {
        addFieldToConvert { .expiry(value) }
    }

    @discardableResult
    private func addFieldToConvert(_ converter: @escaping FieldConverter) -> Self {
        fieldsToConvert.append(converter)
        return self
    }

    public func build() throws -> StatementSubmitParameters {
        let unsignedStatement = try fieldsToConvert.map { fieldClosure in
            try fieldClosure()
        }

        let dataForProof = try unsignedStatement.deriveProofData()
        let proof = try signer.sign(dataForProof)
        let proofField = StatementField.proof(proof)

        let encodedStatement = try ([proofField] + unsignedStatement).encodeForStore()

        return StatementSubmitParameters(encodedStatement: encodedStatement)
    }
}
