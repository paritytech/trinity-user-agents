import Foundation
import Products
import AsyncExtensions

// MARK: - Local Storage

extension ProductsNativeApi {
    func localStorageRead(key: String) async throws -> String? {
        await localStorage.read(key: key)
    }

    func localStorageWrite(key: String, value: String) async throws {
        await localStorage.write(key: key, value: value)
    }

    func localStorageClear(key: String) async throws {
        await localStorage.clear(key: key)
    }

    func subscribeLocalStorage(key: String) -> AnyAsyncSequence<String?> {
        localStorage.subscribe(key: key)
    }
}
