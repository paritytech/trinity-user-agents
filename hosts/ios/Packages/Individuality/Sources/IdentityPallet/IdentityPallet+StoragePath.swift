import Foundation
import SubstrateSdk

public extension IdentityPallet {
    static var identityPath: StorageCodingPath {
        .init(moduleName: name, itemName: "IdentityOf")
    }

    static var usernameInfoPath: StorageCodingPath {
        .init(moduleName: name, itemName: "UsernameInfoOf")
    }

    static var usernamePath: StorageCodingPath {
        .init(moduleName: name, itemName: "UsernameOf")
    }

    static var personIdentitiesPath: StorageCodingPath {
        .init(moduleName: name, itemName: "PersonIdentities")
    }
}
