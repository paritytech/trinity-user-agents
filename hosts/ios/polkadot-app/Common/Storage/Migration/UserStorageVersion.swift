import Foundation

enum UserStorageVersion: String, CaseIterable {
    case version41 = "UserDataModel41"
    case version42 = "UserDataModel42"
    case version43 = "UserDataModel43"
    case version44 = "UserDataModel44"
    case version45 = "UserDataModel45"
    case version46 = "UserDataModel46"
    case version47 = "UserDataModel47"
    case version48 = "UserDataModel48"
    case version49 = "UserDataModel49"
    case version50 = "UserDataModel50"
    case version51 = "UserDataModel51"
    case version52 = "UserDataModel52"
    /// `polkadot-ios-community` is filling the same sequence, and the tree
    /// under `hosts/ios` is a snapshot of it, so a refresh can bring a version
    /// of its own under this name. That conflicts on the file, which is where
    /// someone folds the two into one.
    ///
    /// What they fold has to hold both sides. CoreData matches a store by its
    /// entities rather than by the name of the version, so two models sharing
    /// this name and differing in what they hold leave a migrated device with
    /// no model it matches, which ends in the migrator's `fatalError`.
    case version53 = "UserDataModel53"

    // swiftlint:disable:next cyclomatic_complexity
    func nextVersion() -> UserStorageVersion? {
        switch self {
        case .version41:
            .version42
        case .version42:
            .version43
        case .version43:
            .version44
        case .version44:
            .version45
        case .version45:
            .version46
        case .version46:
            .version47
        case .version47:
            .version48
        case .version48:
            .version49
        case .version49:
            .version50
        case .version50:
            .version51
        case .version51:
            .version52
        case .version52:
            .version53
        case .version53:
            nil
        }
    }
}
