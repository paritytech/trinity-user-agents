import DesignSystem
import Products
import AsyncExtensions
import UIKit

extension ProductsNativeApi {
    func subscribeTheme() async -> AnyAsyncSequence<ProductTheme> {
        await MainActor.run {
            themeManager.observeTheme()
                .map { theme in
                    await MainActor.run { Self.makeProductTheme(from: theme) }
                }
                .eraseToAnyAsyncSequence()
        }
    }
}

private extension ProductsNativeApi {
    static func makeProductTheme(from theme: Theme) -> ProductTheme {
        let variant: ProductTheme.Variant = theme.colors.bgSurfaceMain.isLight ? .light : .dark

        return ProductTheme(name: theme.id, variant: variant)
    }
}
