import SwiftUI
import DesignSystem

public enum ChainConnectionState: Hashable {
    case connected
    case connecting
    case offline
}

public enum ChainStatusIcon: Hashable {
    case people
    case bulletin
    case assetHub
    case statementStore
}

public struct ChainConnectionStatusViewModel: Hashable, Identifiable {
    public let id: String
    public let title: String
    public let state: ChainConnectionState
    public let stateTitle: String
    public let icon: ChainStatusIcon
    public let indication: ChainStatusIndication
    public let liveness: Double?
    public let expectedBlockSeconds: Double
    public let showsChainMetrics: Bool

    public init(
        id: String,
        title: String,
        state: ChainConnectionState,
        stateTitle: String,
        icon: ChainStatusIcon,
        indication: ChainStatusIndication,
        liveness: Double?,
        expectedBlockSeconds: Double,
        showsChainMetrics: Bool = true
    ) {
        self.id = id
        self.title = title
        self.state = state
        self.stateTitle = stateTitle
        self.icon = icon
        self.indication = indication
        self.liveness = liveness
        self.expectedBlockSeconds = expectedBlockSeconds
        self.showsChainMetrics = showsChainMetrics
    }

    public func withIndication(
        _ indication: ChainStatusIndication,
        liveness: Double?
    ) -> ChainConnectionStatusViewModel {
        ChainConnectionStatusViewModel(
            id: id,
            title: title,
            state: state,
            stateTitle: stateTitle,
            icon: icon,
            indication: indication,
            liveness: liveness,
            expectedBlockSeconds: expectedBlockSeconds,
            showsChainMetrics: showsChainMetrics
        )
    }
}
