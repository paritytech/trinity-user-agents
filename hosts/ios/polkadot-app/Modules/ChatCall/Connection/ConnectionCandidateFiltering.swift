import Foundation

/// Decides whether a local or remote ICE candidate should participate in connectivity checks.
/// Implementations are applied symmetrically: outgoing candidates are dropped before they
/// reach the signaling channel; incoming candidates are dropped before they enter the local
/// ICE agent. Pass-through (no filtering) is the default — callers opt in by injecting a
/// concrete filter.
protocol ConnectionCandidateFiltering {
    func shouldAccept(_ candidate: PeerConnectionCandidate) -> Bool
}

/// Drops candidates with TCP transport and `typ host`. They almost never succeed across
/// NAT (both peers would need to accept inbound TCP), yet on devices with many network
/// interfaces they make up ~50% of generated candidates and inflate ICE pair counts
/// quadratically.
struct TcpHostCandidateFilter: ConnectionCandidateFiltering {
    func shouldAccept(_ candidate: PeerConnectionCandidate) -> Bool {
        !candidate.isTCPHost
    }
}
