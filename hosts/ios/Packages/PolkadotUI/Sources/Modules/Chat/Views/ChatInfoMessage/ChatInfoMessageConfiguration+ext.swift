import DesignSystem
import UIKit
internal import SnapKit

public extension ChatInfoMessageConfiguration {
    static func youAdded(username: String) -> ChatInfoMessageConfiguration {
        infoActionConfiguration(
            fullText: String(localized: .chatYouAddedContact(username: username)),
            username: username,
            icon: UIImage(resource: .peopleOutline)
        )
    }

    static func youAdded(by username: String) -> ChatInfoMessageConfiguration {
        infoActionConfiguration(
            fullText: String(localized: .chatContactAddedYou(username: username)),
            username: username,
            icon: UIImage(resource: .peopleOutline)
        )
    }

    static func leftChat(username: String) -> ChatInfoMessageConfiguration {
        infoActionConfiguration(
            fullText: String(localized: .chatPeerLeft(username: username)),
            username: username,
            icon: UIImage(resource: .leavechat)
        )
    }

    static func youLeft() -> ChatInfoMessageConfiguration {
        infoActionConfiguration(
            fullText: String(localized: .chatYouLeft),
            username: "",
            icon: UIImage(resource: .leavechat)
        )
    }

    static func chatRequested() -> ChatInfoMessageConfiguration {
        infoActionConfiguration(
            fullText: String(localized: .chatRequestSent),
            username: "",
            icon: UIImage(resource: .chatRequestMessageIcon)
        )
    }

    static func chatRequestAccepted(by username: String) -> ChatInfoMessageConfiguration {
        infoActionConfiguration(
            fullText: String(localized: .chatRequestAcceptedByPeer(username: username)),
            username: username,
            icon: UIImage(resource: .chatRequestMessageIcon)
        )
    }

    static func chatRequestAcceptedByYou() -> ChatInfoMessageConfiguration {
        infoActionConfiguration(
            fullText: String(localized: .chatRequestAcceptedByYou),
            username: "",
            icon: UIImage(resource: .chatRequestMessageIcon)
        )
    }

    static func newMessages() -> ChatInfoMessageConfiguration {
        let text = NSAttributedString(
            string: String(localized: .chatNewMessages),
            attributes: [
                .font: UIFont.titleSmall,
                .foregroundColor: UIColor.fgTertiary
            ]
        )
        return ChatInfoMessageConfiguration(
            attributedText: text,
            contentInsets: .init(vertical: 12),
            showDividers: true
        )
    }
}

private extension ChatInfoMessageConfiguration {
    static func infoActionConfiguration(
        fullText: String,
        username: String,
        icon: UIImage
    ) -> ChatInfoMessageConfiguration {
        let textAttributes = LabelStyle.body14Regular().attributes(
            for: .center,
            textColor: UIColor.fgSecondary
        )
        let boldTextAttributes = LabelStyle.body14SemiBold().attributes(
            for: .center,
            textColor: UIColor.fgSecondary
        )

        let image = icon.withTintColor(UIColor.fgSecondary, renderingMode: .alwaysOriginal)

        let text = makeIconTextAttributedString(
            image: image,
            text: fullText,
            boldText: username,
            textAttributes: textAttributes,
            boldTextAttributes: boldTextAttributes,
            imageToTextSpacing: 8
        )

        return ChatInfoMessageConfiguration(
            attributedText: text,
            contentInsets: .init(vertical: 4)
        )
    }
}

/// Position of the icon attachment relative to the text.
private enum AttachmentPosition {
    case beginning
    case end
}

/// Builds an attributed string with an icon and text (optionally with a bold substring).
///
/// - Parameters:
///   - image: The icon to attach.
///   - position: Where to place the icon: `.beginning` (before text) or `.end` (after text).
///   - imageToTextSpacing: Horizontal space between the icon and the text.
/// - Returns: A single attributed string with icon and styled text.
private func makeIconTextAttributedString(
    image: UIImage,
    position: AttachmentPosition = .beginning,
    text: String,
    boldText: String,
    textAttributes: [NSAttributedString.Key: Any],
    boldTextAttributes: [NSAttributedString.Key: Any],
    imageToTextSpacing: CGFloat,
) -> NSAttributedString {
    let fallbackFont = UIFont.preferredFont(forTextStyle: .body)
    let baseFont = (textAttributes[.font] as? UIFont)
        ?? (boldTextAttributes[.font] as? UIFont)
        ?? fallbackFont

    let attachment = NSTextAttachment()
    attachment.image = image
    let imageSize = image.size
    let yOffset = (baseFont.capHeight - imageSize.height) / 2
    attachment.bounds = CGRect(x: 0, y: yOffset, width: imageSize.width, height: imageSize.height)

    let resultString = NSMutableAttributedString()

    let configureBody = {
        if !text.isEmpty {
            resultString.append(NSAttributedString(string: text, attributes: textAttributes))
        }
        if !boldText.isEmpty {
            resultString.applyAttributes(boldTextAttributes, toOccurrencesOf: boldText)
        }
    }

    switch position {
    case .beginning:
        resultString.append(NSAttributedString(attachment: attachment))
        if imageToTextSpacing > 0 {
            resultString.append(makeAttachmentSpacer(width: imageToTextSpacing, baselineAlignedTo: baseFont))
        }
        configureBody()
    case .end:
        configureBody()
        if imageToTextSpacing > 0 {
            resultString.append(makeAttachmentSpacer(width: imageToTextSpacing, baselineAlignedTo: baseFont))
        }
        resultString.append(NSAttributedString(attachment: attachment))
    }

    return resultString
}

/// Creates an empty attachment that occupies `width` points and aligns to the given font.
private func makeAttachmentSpacer(width: CGFloat, baselineAlignedTo font: UIFont) -> NSAttributedString {
    let transparentPixel = UIImage()
    let spacer = NSTextAttachment()
    spacer.image = transparentPixel
    let height: CGFloat = max(1, font.capHeight)
    let yOffset = (font.capHeight - height) / 2
    spacer.bounds = CGRect(x: 0, y: yOffset, width: width, height: height)
    return NSAttributedString(attachment: spacer)
}
