import Foundation

enum SubstrateStorageVersion: String, CaseIterable {
    case version1 = "SubstrateDataModel"
    case version2 = "SubstrateDataModel2"
    case version3 = "SubstrateDataModel3"
    case version4 = "SubstrateDataModel4"

    static var current: SubstrateStorageVersion {
        guard let currentVersion = allCases.last else {
            fatalError("Unable to find current storage version")
        }

        return currentVersion
    }

    func nextVersion() -> SubstrateStorageVersion? {
        switch self {
        case .version1:
            .version2
        case .version2:
            .version3
        case .version3:
            .version4
        case .version4:
            nil
        }
    }
}
