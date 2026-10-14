import Foundation
import Operation_iOS
import SubstrateSdk

protocol XTokensMetadataQueryFactoryProtocol {
    func createModuleNameResolutionWrapper(
        for runtimeProvider: RuntimeCodingServiceProtocol
    ) -> CompoundOperationWrapper<String>
}

final class XTokensMetadataQueryFactory: XcmBaseMetadataQueryFactory, XTokensMetadataQueryFactoryProtocol {
    func createModuleNameResolutionWrapper(
        for runtimeProvider: RuntimeCodingServiceProtocol
    ) -> CompoundOperationWrapper<String> {
        createModuleNameResolutionWrapper(
            for: runtimeProvider,
            possibleNames: XTokens.possibleModuleNames
        )
    }
}

enum XTokens {
    static var possibleModuleNames: [String] { ["XTokens", "Xtokens"] }
}
