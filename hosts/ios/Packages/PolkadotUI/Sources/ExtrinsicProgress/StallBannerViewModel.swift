import Foundation

public struct StallBannerViewModel: Equatable {
    public struct Stage: Equatable, Identifiable {
        public enum State: Equatable {
            case done
            case current
            case upcoming
            case skipped
            case failed
        }

        public let id: String
        public let depth: Int
        public let text: String
        public let state: State
        public let detail: String?

        public init(
            id: String,
            depth: Int = 0,
            text: String,
            state: State,
            detail: String? = nil
        ) {
            self.id = id
            self.depth = depth
            self.text = text
            self.state = state
            self.detail = detail
        }
    }

    public struct Transaction: Equatable, Identifiable {
        public let id: UUID
        public let title: String
        public let chainName: String?
        public let stages: [Stage]
        public let dismissAccessibilityLabel: String?

        public init(
            id: UUID,
            title: String,
            chainName: String? = nil,
            stages: [Stage],
            dismissAccessibilityLabel: String? = nil
        ) {
            self.id = id
            self.title = title
            self.chainName = chainName
            self.stages = stages
            self.dismissAccessibilityLabel = dismissAccessibilityLabel
        }
    }

    public let transactions: [Transaction]
    public let overflowText: String?

    public init(transactions: [Transaction], overflowText: String?) {
        self.transactions = transactions
        self.overflowText = overflowText
    }
}
