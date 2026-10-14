import Testing
@testable import PolkadotUI

struct ChainStatusIndicationTests {
    @Test(
        "Each connection state resolves to its indication",
        arguments: [
            (ChainConnectionState.connected, ChainStatusIndication.normal),
            (ChainConnectionState.connecting, ChainStatusIndication.dead),
            (ChainConnectionState.offline, ChainStatusIndication.dead)
        ]
    )
    func resolvesStateToIndication(state: ChainConnectionState, expectedIndication: ChainStatusIndication) {
        #expect(ChainStatusIndication.resolve(state: state, liveness: nil) == expectedIndication)
    }

    @Test("Nil liveness resolves to normal when connected")
    func nilLivenessIsNormal() {
        #expect(ChainStatusIndication.resolve(state: .connected, liveness: nil) == .normal)
    }

    @Test(
        "Liveness just below threshold is outage",
        arguments: [0.0, 0.1, 0.5, 0.8]
    )
    func livenessBelowThresholdIsOutage(liveness: Double) {
        let indication = ChainStatusIndication.resolve(state: .connected, liveness: liveness)
        guard case let .outage(value) = indication else {
            Issue.record("Expected outage but got \(indication)")
            return
        }
        #expect(value == liveness)
    }

    @Test(
        "Liveness at or above threshold is normal",
        arguments: [5.0 / 6.0, 0.85, 1.0]
    )
    func livenessAtOrAboveThresholdIsNormal(liveness: Double) {
        #expect(ChainStatusIndication.resolve(state: .connected, liveness: liveness) == .normal)
    }

    @Test("Dead beats outage")
    func deadBeatsOutage() {
        let dead = ChainStatusIndication.resolve(state: .offline, liveness: nil)
        let outage = ChainStatusIndication.resolve(state: .connected, liveness: 0.5)
        #expect(dead == .dead)
        guard case .outage = outage else {
            Issue.record("Expected outage but got \(outage)")
            return
        }
    }
}
