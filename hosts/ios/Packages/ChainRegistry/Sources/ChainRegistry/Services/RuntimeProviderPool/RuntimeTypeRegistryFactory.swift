import Foundation
import SubstrateSdk
import SDKLogger

public protocol SignedExtensionCodersFactoryProtocol {
    func createCoders(for metadata: RuntimeMetadataProtocol) -> [TransactionExtensionCoding]
}

public struct RuntimeTypeRegistryInfo {
    public let runtimeMetadata: RuntimeMetadataProtocol
    public let typeRegistryCatalog: TypeRegistryCatalogProtocol
}

public enum RuntimeTypeRegistryFactoryError: Error {
    case unsupportedRuntimeVersion
}

public protocol RuntimeTypeRegistryFactoryProtocol {
    func createForMetadataAndDefaultTyping(
        chain: RuntimeProviderChain,
        runtimeMetadataItem: RuntimeMetadataItem
    ) throws -> RuntimeTypeRegistryInfo

    func createForChainTypes(
        chain: RuntimeProviderChain,
        runtimeMetadataItem: RuntimeMetadataItem,
        chainTypes: Data
    ) throws -> RuntimeTypeRegistryInfo

    func createForCommonTypes(
        chain: RuntimeProviderChain,
        runtimeMetadataItem: RuntimeMetadataItem,
        commonTypes: Data
    ) throws -> RuntimeTypeRegistryInfo

    func createForCommonAndChainTypes(
        chain: RuntimeProviderChain,
        runtimeMetadataItem: RuntimeMetadataItem,
        commonTypes: Data,
        chainTypes: Data
    ) throws -> RuntimeTypeRegistryInfo
}

public final class RuntimeTypeRegistryFactory: RuntimeTypeRegistryFactoryProtocol {
    public let logger: SDKLoggerProtocol
    public let signedExtensionCodersFactory: SignedExtensionCodersFactoryProtocol

    public init(
        signedExtensionCodersFactory: SignedExtensionCodersFactoryProtocol,
        logger: SDKLoggerProtocol
    ) {
        self.signedExtensionCodersFactory = signedExtensionCodersFactory
        self.logger = logger
    }

    private func createRuntimeContainer(
        from runtimeMetadataItem: RuntimeMetadataItem
    ) throws -> RuntimeMetadataContainer {
        if runtimeMetadataItem.opaque {
            return try RuntimeMetadataContainer.createFromOpaque(data: runtimeMetadataItem.metadata)
        } else {
            let decoder = try ScaleDecoder(data: runtimeMetadataItem.metadata)
            return try RuntimeMetadataContainer(scaleDecoder: decoder)
        }
    }

    private func createForPostV14MetadataAndDefaultTyping(
        _ post14Metadata: PostV14RuntimeMetadataProtocol,
        chain: RuntimeProviderChain
    ) throws -> RuntimeTypeRegistryInfo {
        let augmentationFactory = RuntimeAugmentationFactory()

        let result = !chain.isEthereumBased ? augmentationFactory.createSubstrateAugmentation(for: post14Metadata) :
            augmentationFactory.createEthereumBasedAugmentation(for: post14Metadata)

        let signedExtensionFactory = signedExtensionCodersFactory

        let catalog = try TypeRegistryCatalog.createFromSiDefinition(
            runtimeMetadata: post14Metadata,
            additionalNodes: result.additionalNodes.nodes,
            customExtensions: signedExtensionFactory.createCoders(for: post14Metadata),
            customTypeMapper: CustomSiMappers.all,
            customNameMapper: ScaleInfoCamelCaseMapper()
        )

        if !result.additionalNodes.notMatch.isEmpty {
            logger.warning("No \(chain.name) type matching: \(result.additionalNodes.notMatch)")
        } else {
            logger.debug("Types matching succeed for \(chain.name)")
        }

        return RuntimeTypeRegistryInfo(runtimeMetadata: post14Metadata, typeRegistryCatalog: catalog)
    }
}

public extension RuntimeTypeRegistryFactory {
    func createForMetadataAndDefaultTyping(
        chain: RuntimeProviderChain,
        runtimeMetadataItem: RuntimeMetadataItem
    ) throws -> RuntimeTypeRegistryInfo {
        let runtimeMetadataContainer = try createRuntimeContainer(from: runtimeMetadataItem)

        switch runtimeMetadataContainer.runtimeMetadata {
        case let .v14(metadata):
            return try createForPostV14MetadataAndDefaultTyping(metadata, chain: chain)
        case let .v15(metadata):
            return try createForPostV14MetadataAndDefaultTyping(metadata, chain: chain)
        case let .v16(metadata):
            return try createForPostV14MetadataAndDefaultTyping(metadata, chain: chain)
        case .v13:
            throw RuntimeTypeRegistryFactoryError.unsupportedRuntimeVersion
        }
    }

    func createForChainTypes(
        chain _: RuntimeProviderChain,
        runtimeMetadataItem: RuntimeMetadataItem,
        chainTypes: Data
    ) throws -> RuntimeTypeRegistryInfo {
        let runtimeMetadataContainer = try createRuntimeContainer(from: runtimeMetadataItem)

        let signedExtensionFactory = signedExtensionCodersFactory

        switch runtimeMetadataContainer.runtimeMetadata {
        case let .v13(metadata):
            let catalog = try TypeRegistryCatalog.createFromTypeDefinition(
                chainTypes,
                runtimeMetadata: metadata,
                customExtensions: signedExtensionFactory.createCoders(for: metadata)
            )

            return RuntimeTypeRegistryInfo(runtimeMetadata: metadata, typeRegistryCatalog: catalog)
        case let .v14(metadata):
            let catalog = try TypeRegistryCatalog.createFromSiDefinition(
                versioningData: chainTypes,
                runtimeMetadata: metadata,
                customExtensions: signedExtensionFactory.createCoders(for: metadata),
                customTypeMapper: CustomSiMappers.all,
                customNameMapper: ScaleInfoCamelCaseMapper()
            )

            return RuntimeTypeRegistryInfo(runtimeMetadata: metadata, typeRegistryCatalog: catalog)
        case let .v15(metadata):
            let catalog = try TypeRegistryCatalog.createFromSiDefinition(
                versioningData: chainTypes,
                runtimeMetadata: metadata,
                customExtensions: signedExtensionFactory.createCoders(for: metadata),
                customTypeMapper: CustomSiMappers.all,
                customNameMapper: ScaleInfoCamelCaseMapper()
            )

            return RuntimeTypeRegistryInfo(runtimeMetadata: metadata, typeRegistryCatalog: catalog)
        case let .v16(metadata):
            let catalog = try TypeRegistryCatalog.createFromSiDefinition(
                versioningData: chainTypes,
                runtimeMetadata: metadata,
                customExtensions: signedExtensionFactory.createCoders(for: metadata),
                customTypeMapper: CustomSiMappers.all,
                customNameMapper: ScaleInfoCamelCaseMapper()
            )

            return RuntimeTypeRegistryInfo(runtimeMetadata: metadata, typeRegistryCatalog: catalog)
        }
    }

    func createForCommonTypes(
        chain _: RuntimeProviderChain,
        runtimeMetadataItem: RuntimeMetadataItem,
        commonTypes: Data
    ) throws -> RuntimeTypeRegistryInfo {
        let runtimeMetadataContainer = try createRuntimeContainer(from: runtimeMetadataItem)
        let signedExtensionFactory = signedExtensionCodersFactory

        switch runtimeMetadataContainer.runtimeMetadata {
        case let .v13(metadata):
            let catalog = try TypeRegistryCatalog.createFromTypeDefinition(
                commonTypes,
                runtimeMetadata: metadata,
                customExtensions: signedExtensionFactory.createCoders(for: metadata)
            )

            return RuntimeTypeRegistryInfo(runtimeMetadata: metadata, typeRegistryCatalog: catalog)
        case let .v14(metadata):
            let catalog = try TypeRegistryCatalog.createFromSiDefinition(
                versioningData: commonTypes,
                runtimeMetadata: metadata,
                customExtensions: signedExtensionFactory.createCoders(for: metadata),
                customTypeMapper: CustomSiMappers.all,
                customNameMapper: ScaleInfoCamelCaseMapper()
            )

            return RuntimeTypeRegistryInfo(runtimeMetadata: metadata, typeRegistryCatalog: catalog)
        case let .v15(metadata):
            let catalog = try TypeRegistryCatalog.createFromSiDefinition(
                versioningData: commonTypes,
                runtimeMetadata: metadata,
                customExtensions: signedExtensionFactory.createCoders(for: metadata),
                customTypeMapper: CustomSiMappers.all,
                customNameMapper: ScaleInfoCamelCaseMapper()
            )

            return RuntimeTypeRegistryInfo(runtimeMetadata: metadata, typeRegistryCatalog: catalog)
        case let .v16(metadata):
            let catalog = try TypeRegistryCatalog.createFromSiDefinition(
                versioningData: commonTypes,
                runtimeMetadata: metadata,
                customExtensions: signedExtensionFactory.createCoders(for: metadata),
                customTypeMapper: CustomSiMappers.all,
                customNameMapper: ScaleInfoCamelCaseMapper()
            )

            return RuntimeTypeRegistryInfo(runtimeMetadata: metadata, typeRegistryCatalog: catalog)
        }
    }

    func createForCommonAndChainTypes(
        chain _: RuntimeProviderChain,
        runtimeMetadataItem: RuntimeMetadataItem,
        commonTypes: Data,
        chainTypes: Data
    ) throws -> RuntimeTypeRegistryInfo {
        let runtimeMetadataContainer = try createRuntimeContainer(from: runtimeMetadataItem)
        let signedExtensionFactory = signedExtensionCodersFactory

        switch runtimeMetadataContainer.runtimeMetadata {
        case let .v13(metadata):
            let catalog = try TypeRegistryCatalog.createFromTypeDefinition(
                commonTypes,
                versioningData: chainTypes,
                runtimeMetadata: metadata,
                customExtensions: signedExtensionFactory.createCoders(for: metadata)
            )

            return RuntimeTypeRegistryInfo(runtimeMetadata: metadata, typeRegistryCatalog: catalog)
        case let .v14(metadata):
            let catalog = try TypeRegistryCatalog.createFromSiDefinition(
                versioningData: chainTypes,
                runtimeMetadata: metadata,
                customExtensions: signedExtensionFactory.createCoders(for: metadata),
                customTypeMapper: CustomSiMappers.all,
                customNameMapper: ScaleInfoCamelCaseMapper()
            )

            return RuntimeTypeRegistryInfo(runtimeMetadata: metadata, typeRegistryCatalog: catalog)
        case let .v15(metadata):
            let catalog = try TypeRegistryCatalog.createFromSiDefinition(
                versioningData: chainTypes,
                runtimeMetadata: metadata,
                customExtensions: signedExtensionFactory.createCoders(for: metadata),
                customTypeMapper: CustomSiMappers.all,
                customNameMapper: ScaleInfoCamelCaseMapper()
            )

            return RuntimeTypeRegistryInfo(runtimeMetadata: metadata, typeRegistryCatalog: catalog)
        case let .v16(metadata):
            let catalog = try TypeRegistryCatalog.createFromSiDefinition(
                versioningData: chainTypes,
                runtimeMetadata: metadata,
                customExtensions: signedExtensionFactory.createCoders(for: metadata),
                customTypeMapper: CustomSiMappers.all,
                customNameMapper: ScaleInfoCamelCaseMapper()
            )

            return RuntimeTypeRegistryInfo(runtimeMetadata: metadata, typeRegistryCatalog: catalog)
        }
    }
}
