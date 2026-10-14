import Foundation
import SubstrateSdk

@propertyWrapper
struct ChainDictionary<K: Decodable & Hashable, V: Decodable>: Decodable {
    let wrappedValue: [K: V]

    init(wrappedValue: [K: V]) {
        self.wrappedValue = wrappedValue
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()

        let values = try container.decode([ChainTuple<K, V>].self)

        wrappedValue = values.reduce(into: [:]) { partialResult, tuple in
            partialResult[tuple.key] = tuple.value
        }
    }
}

struct ChainTuple<K: Decodable, V: Decodable>: Decodable {
    let key: K
    let value: V

    init(from decoder: any Decoder) throws {
        var container = try decoder.unkeyedContainer()
        key = try container.decode(K.self)
        value = try container.decode(V.self)
    }
}
