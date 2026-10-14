import Foundation

public extension Statement {
    static func fromStatementFields(
        topics: [Data],
        channel: Data?,
        expiry: UInt64?,
        data: Data?,
        proof: StatementProof?
    ) throws -> Self {
        var fields: Statement = []

        if let expiry {
            fields.append(.expiry(expiry))
        }

        if let channel {
            fields.append(.channel(channel))
        }

        for (index, topic) in topics.enumerated() {
            switch index {
            case 0: fields.append(.topic1(topic))
            case 1: fields.append(.topic2(topic))
            case 2: fields.append(.topic3(topic))
            case 3: fields.append(.topic4(topic))
            default: break
            }
        }

        if let data {
            let encoded = try data.scaleEncoded()
            fields.append(.scaleEncodedPayload(encoded))
        }

        if let proof {
            fields.append(.proof(proof))
        }

        return fields.sortedByIndex()
    }

    func encodeForStore() throws -> Data {
        try sortedByIndex().scaleEncoded()
    }
}

extension Statement: StatementSubmitParametersBuilding {
    public func build() throws -> StatementSubmitParameters {
        let encodedStatement = try encodeForStore()

        return StatementSubmitParameters(encodedStatement: encodedStatement)
    }
}
