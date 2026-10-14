import Foundation

public enum TopicFilter {
    case anyTopic
    case matchAll([Data])
    case matchAny([Data])
}

extension TopicFilter: Encodable {
    enum CodingKeys: String, CodingKey {
        case matchAll
        case matchAny
    }

    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .anyTopic:
            var container = encoder.singleValueContainer()
            try container.encode("Any")
        case let .matchAll(dataList):
            var container = encoder.container(keyedBy: CodingKeys.self)

            let hexTopics = dataList.map { $0.toHex(includePrefix: true) }
            try container.encode(hexTopics, forKey: .matchAll)
        case let .matchAny(dataList):
            var container = encoder.container(keyedBy: CodingKeys.self)

            let hexTopics = dataList.map { $0.toHex(includePrefix: true) }
            try container.encode(hexTopics, forKey: .matchAny)
        }
    }
}

public extension TopicFilter {
    func matches(topics: Set<Data>) -> Bool {
        switch self {
        case .anyTopic:
            !topics.isEmpty
        case let .matchAll(filterTopics):
            topics.isSubset(of: Set(filterTopics))
        case let .matchAny(filterTopics):
            topics.contains { filterTopics.contains($0) }
        }
    }
}
