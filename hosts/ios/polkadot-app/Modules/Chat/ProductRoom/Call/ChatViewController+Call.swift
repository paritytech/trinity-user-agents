import SnapKit
import SwiftUI
import UIKit

/// The ringing banner and the call bar of a product room, over the top of the messages (07-LATER §11.2).
extension ChatViewController {
    func didReceive(callBar: ChatRoomCallBarModel) {
        let existing = children.compactMap { $0 as? UIHostingController<ChatRoomCallBar> }.first
        if let existing {
            guard existing.rootView.model !== callBar else { return }
            existing.rootView = ChatRoomCallBar(model: callBar)
            return
        }

        let hosting = UIHostingController(rootView: ChatRoomCallBar(model: callBar))
        hosting.sizingOptions = .intrinsicContentSize
        hosting.view.backgroundColor = .clear
        addChild(hosting)
        view.addSubview(hosting.view)
        hosting.view.snp.makeConstraints { make in
            make.top.equalTo(view.safeAreaLayoutGuide.snp.top).offset(8)
            make.leading.trailing.equalToSuperview()
        }
        hosting.didMove(toParent: self)
    }
}
