import UIKit

@MainActor
extension SPAPresenterProtocol {
    func makeMenuElements() -> [UIMenuElement] {
        var elements: [UIMenuElement] = []

        if hasChatEntry() {
            elements.append(UIAction(
                title: String(localized: .spaActionOpenChat),
                image: UIImage(systemName: "bubble.left")
            ) { [weak self] _ in
                self?.didTapOpenChat()
            })
        }

        elements.append(UIAction(
            title: String(localized: .spaActionRefresh),
            image: UIImage(systemName: "arrow.clockwise")
        ) { [weak self] _ in
            self?.didTapRefresh()
        })

        elements.append(UIAction(
            title: String(localized: .spaActionShare),
            image: UIImage(systemName: "square.and.arrow.up")
        ) { [weak self] _ in
            self?.didTapShare()
        })

        let closeApp = UIAction(
            title: String(localized: .spaActionCloseApp),
            image: UIImage(systemName: "xmark")
        ) { [weak self] _ in
            self?.didTapClose()
        }
        elements.append(UIMenu(options: .displayInline, children: [closeApp]))

        return elements
    }
}
