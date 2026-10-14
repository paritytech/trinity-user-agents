import Foundation

/// Why setup gave up. Determines the failure screen's copy.
enum RootSetupFailureKind: Equatable {
    /// The network path was unsatisfied when setup gave up.
    case connectivity
    /// Setup reached a stage whose configuration is unusable, so retrying cannot help until it is fixed remotely.
    case configuration(RootSetupStage)
    case unknown
}
