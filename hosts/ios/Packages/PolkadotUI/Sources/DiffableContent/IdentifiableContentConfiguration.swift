import UIKit

// MARK: - Identifiable UIContentConfiguration

public struct IdentifiableContentConfiguration<ID: Hashable, Configuration: HashableContentConfiguration> {
    let id: ID
    let configuration: Configuration

    public init(id: ID, configuration: Configuration) {
        self.id = id
        self.configuration = configuration
    }

    public init(_ id: ID, _ configuration: Configuration) {
        self.id = id
        self.configuration = configuration
    }
}

public struct IdentifiableAnyContentConfiguration<ID: Hashable> {
    public let id: ID
    let configuration: any HashableContentConfiguration

    public init(id: ID, configuration: any HashableContentConfiguration) {
        self.id = id
        self.configuration = configuration
    }

    public init(_ id: ID, _ configuration: any HashableContentConfiguration) {
        self.id = id
        self.configuration = configuration
    }
}

public extension Array where Element: HashableContentConfiguration {
    func identified<ID: Hashable>(by makeID: (Element) -> ID) -> [IdentifiableContentConfiguration<ID, Element>] {
        map { IdentifiableContentConfiguration(makeID($0), $0) }
    }

    func identified<ID: Hashable>(by keyPath: KeyPath<Element, ID>) -> [IdentifiableContentConfiguration<ID, Element>] {
        map { IdentifiableContentConfiguration($0[keyPath: keyPath], $0) }
    }

    func identifiedByUUIDs() -> [IdentifiableContentConfiguration<String, Element>] {
        map { IdentifiableContentConfiguration(UUID().uuidString, $0) }
    }
}

public extension Array where Element: HashableContentConfiguration {
    func identified<ID: Hashable>(by makeID: (Element) -> ID) -> [IdentifiableAnyContentConfiguration<ID>] {
        map { IdentifiableAnyContentConfiguration(makeID($0), $0) }
    }

    func identified<ID: Hashable>(by keyPath: KeyPath<Element, ID>) -> [IdentifiableAnyContentConfiguration<ID>] {
        map { IdentifiableAnyContentConfiguration($0[keyPath: keyPath], $0) }
    }

    func identifiedByUUIDs() -> [IdentifiableAnyContentConfiguration<String>] {
        map { IdentifiableAnyContentConfiguration(UUID().uuidString, $0) }
    }
}
