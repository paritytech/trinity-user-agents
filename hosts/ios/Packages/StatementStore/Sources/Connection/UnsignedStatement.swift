import Foundation
import SubstrateSdk

/// Statement fields without a proof, for callers that pick the signer only at submission time.
public final class UnsignedStatement {
    typealias FieldConverter = () throws -> StatementField

    private var fieldsToConvert = [FieldConverter]()

    public init() {}

    @discardableResult
    public func addChannel(_ value: StatementFixedFieldConvertible) -> Self {
        addFieldToConvert { try .channel(value.fixedStatementFieldData()) }
    }

    @discardableResult
    public func addTopic1(_ value: StatementFixedFieldConvertible) -> Self {
        addFieldToConvert { try .topic1(value.fixedStatementFieldData()) }
    }

    @discardableResult
    public func addTopic2(_ value: StatementFixedFieldConvertible) -> Self {
        addFieldToConvert { try .topic2(value.fixedStatementFieldData()) }
    }

    @discardableResult
    public func addTopic3(_ value: StatementFixedFieldConvertible) -> Self {
        addFieldToConvert { try .topic3(value.fixedStatementFieldData()) }
    }

    @discardableResult
    public func addTopic4(_ value: StatementFixedFieldConvertible) -> Self {
        addFieldToConvert { try .topic4(value.fixedStatementFieldData()) }
    }

    @discardableResult
    public func addScaleEncodedPayload(_ value: Data) -> Self {
        addFieldToConvert { .scaleEncodedPayload(value) }
    }

    @discardableResult
    public func addExpiry(_ value: UInt64) -> Self {
        addFieldToConvert { .expiry(value) }
    }

    public func signed(by signer: StatementStoreSigning) -> StatementSubmitParametersBuilding {
        SignedStatementParametersBuilder(statement: self, signer: signer)
    }

    /// Encoded size once signed with an sr25519 proof, to check it against the store's limit before signing.
    public func signedSize() throws -> Int {
        let placeholderProof = StatementProof.sr25519(
            signature: Data(count: StatementProof.signatureSize),
            signer: Data(count: StatementProof.signerSize)
        )

        return try ([StatementField.proof(placeholderProof)] + fields()).encodeForStore().count
    }
}

extension UnsignedStatement {
    func fields() throws -> [StatementField] {
        try fieldsToConvert.map { try $0() }
    }
}

private extension UnsignedStatement {
    @discardableResult
    func addFieldToConvert(_ converter: @escaping FieldConverter) -> Self {
        fieldsToConvert.append(converter)
        return self
    }
}

private struct SignedStatementParametersBuilder: StatementSubmitParametersBuilding {
    let statement: UnsignedStatement
    let signer: StatementStoreSigning

    func build() throws -> StatementSubmitParameters {
        let unsignedFields = try statement.fields()
        let proof = try signer.sign(unsignedFields.deriveProofData())
        let encodedStatement = try ([StatementField.proof(proof)] + unsignedFields).encodeForStore()

        return StatementSubmitParameters(encodedStatement: encodedStatement)
    }
}
