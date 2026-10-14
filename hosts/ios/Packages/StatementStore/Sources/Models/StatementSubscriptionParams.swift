import Foundation

public struct StatementSubscriptionParams: Encodable {
    enum CodingKeys: String, CodingKey {
        case topicFilter = "topic_filter"
    }

    public let topicFilter: TopicFilter
}
