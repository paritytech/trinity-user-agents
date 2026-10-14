@testable import polkadot_app
import Foundation
import SubstrateSdk
import StatementStore

struct MockStatement: Hashable {
    let topics: Set<Data>
    let data: Data

    init(encodedStatement: Data) throws {
        let decoder = try ScaleDecoder(data: encodedStatement)
        let statement = try Statement(scaleDecoder: decoder)

        var foundTopics: Set<Data> = []

        for field in statement {
            switch field {
            case let .topic1(value),
                 let .topic2(value),
                 let .topic3(value),
                 let .topic4(value):
                foundTopics.insert(value)
            default:
                break
            }
        }

        topics = foundTopics
        data = encodedStatement
    }
}
