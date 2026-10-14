import Foundation

public extension XcmUni {
    struct Versioned<Entity> {
        public let entity: Entity
        public let version: Xcm.Version

        public init(entity: Entity, version: Xcm.Version) {
            self.entity = entity
            self.version = version
        }
    }

    typealias VersionedMessage = Versioned<XcmUni.Instructions>
    typealias VersionedAsset = Versioned<XcmUni.Asset>
    typealias VersionedAssets = Versioned<XcmUni.Assets>
    typealias VersionedLocation = Versioned<XcmUni.RelativeLocation>
    typealias VersionedLocatableAsset = Versioned<XcmUni.LocatableAsset>
    typealias VersionedAssetId = Versioned<XcmUni.AssetId>
}

extension XcmUni.Versioned: Equatable where Entity: Equatable {}

public protocol XcmUniVersioned {
    func versioned(_ version: Xcm.Version) -> XcmUni.Versioned<Self>
}

public extension XcmUniVersioned {
    func versioned(_ version: Xcm.Version) -> XcmUni.Versioned<Self> {
        .init(entity: self, version: version)
    }
}

extension XcmUni.RelativeLocation: XcmUniVersioned {}
extension XcmUni.Asset: XcmUniVersioned {}
extension XcmUni.LocatableAsset: XcmUniVersioned {}

extension Array: XcmUniVersioned {}

public extension XcmUni.VersionedAsset {
    func toVersionedAssets() -> XcmUni.VersionedAssets {
        [entity].versioned(version)
    }
}

public extension XcmUni.Versioned {
    func map<U>(_ transformation: (Entity) throws -> U) rethrows -> XcmUni.Versioned<U> {
        let newEntity = try transformation(entity)
        return XcmUni.Versioned(entity: newEntity, version: version)
    }

    func replacingVersion(_ newVersion: Xcm.Version) -> Self {
        XcmUni.Versioned(entity: entity, version: newVersion)
    }
}
