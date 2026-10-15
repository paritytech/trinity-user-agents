import SnapKit
import SwiftUI
import UIKit

/// The app as the alerts of a ringing room see it: the state of the app, the screen in front, and an in-app banner
/// at the top of the key window.
@MainActor
final class ProductRoomCallWindowSurface: ProductRoomCallSurface {
    static let shared = ProductRoomCallWindowSurface()

    /// The UIKit controllers of the banners. The surface keeps them, so that their views continue to work.
    private var banners: [Chat.Id: UIViewController] = [:]

    var isInBackground: Bool {
        UIApplication.shared.applicationState == .background
    }

    func showsRoom(_ chatId: Chat.Id) -> Bool {
        ProductRoomScreens.showsRoom(chatId)
    }

    func showBanner(title: String, subtitle: String, for chatId: Chat.Id, onOpen: @escaping () -> Void) {
        hideBanner(for: chatId)
        guard let window = UIWindow.keyWindow else { return }

        let banner = UIHostingController(rootView: ProductRoomCallBanner(
            title: title,
            subtitle: subtitle,
            onOpen: onOpen
        ))
        banner.view.backgroundColor = .clear
        banner.sizingOptions = .intrinsicContentSize
        window.addSubview(banner.view)
        banner.view.snp.makeConstraints { make in
            make.top.equalTo(window.safeAreaLayoutGuide.snp.top).offset(8)
            make.leading.trailing.equalToSuperview().inset(12)
        }
        banners[chatId] = banner
    }

    func hideBanner(for chatId: Chat.Id) {
        banners.removeValue(forKey: chatId)?.view.removeFromSuperview()
    }

    func openRoom(_ chatId: Chat.Id) {
        guard let productId = chatId.extensionId, let roomId = chatId.roomId,
              let url = URL(string: ProductRoomCallAlerts.roomLink(productId: productId, roomId: roomId)) else {
            return
        }
        DeferredLinkHandler.shared.handle(with: url)
    }
}

/// The in-app banner of a ringing room: the name of the room and "Incoming call". A tap opens the room.
struct ProductRoomCallBanner: View {
    let title: String
    let subtitle: String
    let onOpen: () -> Void

    var body: some View {
        Button(action: onOpen) {
            HStack(spacing: 12) {
                Image(systemName: "phone.arrow.down.left.fill")
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundStyle(Color.fgStaticWhite)
                    .frame(width: 40, height: 40)
                    .background(Color.bgStatusSuccess, in: Circle())
                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .typography(.titleSmall)
                        .foregroundStyle(Color.fgPrimary)
                        .lineLimit(1)
                    Text(subtitle)
                        .typography(.bodySmall)
                        .foregroundStyle(Color.fgSecondary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
            }
            .padding(12)
            .background(Color.bgSurfaceContainer, in: RoundedRectangle(cornerRadius: 16))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
    }
}

/// The room screens of the product rooms that exist now, so the alerts of a ringing room know whether the screen of
/// the room is in front.
@MainActor
enum ProductRoomScreens {
    private final class Entry {
        weak var presenter: ChatProductRoomPresenter?

        init(_ presenter: ChatProductRoomPresenter) {
            self.presenter = presenter
        }
    }

    private static var entries: [Entry] = []

    static func register(_ presenter: ChatProductRoomPresenter) {
        entries = entries.filter { $0.presenter != nil } + [Entry(presenter)]
    }

    /// True when a screen of the room is in a window and not hidden. A navigation stack takes the view of a screen
    /// under its top screen out of the window.
    static func showsRoom(_ chatId: Chat.Id) -> Bool {
        entries.contains { entry in
            guard let presenter = entry.presenter, presenter.chatId == chatId,
                  let view = presenter.view?.controller.viewIfLoaded, view.window != nil else {
                return false
            }
            return !sequence(first: view, next: \.superview).contains { $0.isHidden || $0.alpha == 0 }
        }
    }
}
