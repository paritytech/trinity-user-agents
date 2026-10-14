import Foundation
import SubstrateSdk
import ChainRegistry

struct CustomAssetMapper {
    struct ExtrasToValue<T> {
        let nativeHandler: () -> T
        let statemineHandler: (StatemineAssetExtras) -> T
        let ormlHandler: (OrmlTokenExtras) -> T
        let ormlHydrationEvmHandler: (OrmlTokenExtras) -> T
    }

    struct TypeToValue<T> {
        let nativeHandler: () -> T
        let statemineHandler: () -> T
        let ormlHandler: () -> T
        let ormlHydrationEvmHandler: () -> T
    }

    enum MapperError: Error {
        case invalidJson(_ type: String?)
    }

    let type: String?
    let typeExtras: AssetTypeExtras?

    func mapAssetWithExtras<T>(_ handlers: ExtrasToValue<T>) throws -> T {
        let wrappedType = try AssetType.createOrError(from: type)

        switch wrappedType {
        case .native:
            return handlers.nativeHandler()
        case .statemine:
            guard let wrappedExtras = try? typeExtras?.map(to: StatemineAssetExtras.self) else {
                throw MapperError.invalidJson(type)
            }

            return handlers.statemineHandler(wrappedExtras)
        case .orml:
            guard let wrappedExtras = try? typeExtras?.map(to: OrmlTokenExtras.self) else {
                throw MapperError.invalidJson(type)
            }

            return handlers.ormlHandler(wrappedExtras)
        case .ormlHydrationEvm:
            guard let wrappedExtras = try? typeExtras?.map(to: OrmlTokenExtras.self) else {
                throw MapperError.invalidJson(type)
            }

            return handlers.ormlHandler(wrappedExtras)
        }
    }

    func mapAsset<T>(_ handlers: TypeToValue<T>) throws -> T {
        let wrappedType = try AssetType.createOrError(from: type)

        switch wrappedType {
        case .native:
            return handlers.nativeHandler()
        case .statemine:
            return handlers.statemineHandler()
        case .orml:
            return handlers.ormlHandler()
        case .ormlHydrationEvm:
            return handlers.ormlHydrationEvmHandler()
        }
    }
}
