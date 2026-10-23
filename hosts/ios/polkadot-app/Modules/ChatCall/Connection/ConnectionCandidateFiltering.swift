import Foundation

/// Decides whether a local or remote ICE candidate should participate in connectivity checks.
/// Implementations are applied symmetrically: outgoing candidates are dropped before they
/// reach the signaling channel; incoming candidates are dropped before they enter the local
/// ICE agent. Pass-through (no filtering) is the default — callers opt in by injecting a
/// concrete filter.
protocol ConnectionCandidateFiltering {
    func shouldAccept(_ candidate: PeerConnectionCandidate) -> Bool
}
