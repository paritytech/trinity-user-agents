import Foundation

/// The part of setup a configuration failure is attributed to.
enum RootSetupStage: Equatable {
    case config
    case chains
    case tld
}
