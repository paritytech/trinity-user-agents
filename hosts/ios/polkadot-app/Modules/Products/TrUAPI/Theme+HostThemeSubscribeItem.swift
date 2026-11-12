import DesignSystem
import TrUAPIHost
import UIKit

extension Theme {
    var hostThemeSubscribeItem: HostThemeSubscribeItem {
        HostThemeSubscribeItem(
            name: .custom(id),
            variant: colors.bgSurfaceMain.isLight ? .light : .dark
        )
    }
}
