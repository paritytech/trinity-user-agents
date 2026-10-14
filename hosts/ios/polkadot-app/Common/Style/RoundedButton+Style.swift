import Foundation
import UIKit_iOS
import UIKit
import PolkadotUI

extension RoundedButton {
    func applyBaseFillStyle() {
        roundedBackgroundView?.shadowOpacity = 0.0
        roundedBackgroundView?.strokeColor = .clear
        roundedBackgroundView?.highlightedStrokeColor = .clear
        roundedBackgroundView?.cornerRadius = 12
        // RoundedView fills are CALayer CGColors that don't re-resolve on a live
        // DSThemeTrait switch; reapply them when the theme changes.
        roundedBackgroundView?.enableDynamicColorReapply()

        changesContentOpacityWhenHighlighted = true
    }

    func applyBaseMainStyle() {
        applyBaseFillStyle()

        imageWithTitleView?.titleColor = .fgPrimaryInverted
        imageWithTitleView?.titleFont = .semibold16
    }

    func applyMainStyle() {
        applyBaseMainStyle()

        roundedBackgroundView?.fillColor = .bgActionPrimary
        roundedBackgroundView?.highlightedFillColor = .bgActionPrimaryHover
    }

    func applyBaseSecondaryStyle() {
        applyBaseFillStyle()

        imageWithTitleView?.titleColor = .fgPrimary
        imageWithTitleView?.titleFont = .semibold16
    }

    func applySecondaryStyle(titleFont: UIFont = .semibold16) {
        applyBaseSecondaryStyle()

        imageWithTitleView?.titleFont = titleFont
        roundedBackgroundView?.fillColor = .bgActionTertiary
        roundedBackgroundView?.highlightedFillColor = .bgActionTertiary
    }

    func applyTitleSecondaryStyle() {
        applyTitleStyle()

        imageWithTitleView?.titleColor = .fgSecondary
        imageWithTitleView?.titleFont = .semibold16
        imageWithTitleView?.spacingBetweenLabelAndIcon = 0.0
    }

    func applyTitleTertiaryStyle() {
        applyTitleStyle()

        imageWithTitleView?.titleColor = .fgTertiary
        imageWithTitleView?.titleFont = .semibold16
        imageWithTitleView?.spacingBetweenLabelAndIcon = 0.0
    }

    func applyTitleStyle() {
        roundedBackgroundView?.shadowOpacity = 0.0
        roundedBackgroundView?.fillColor = .clear
        roundedBackgroundView?.highlightedFillColor = .clear
        roundedBackgroundView?.strokeColor = .clear
        roundedBackgroundView?.highlightedStrokeColor = .clear

        changesContentOpacityWhenHighlighted = true

        imageWithTitleView?.titleColor = .fgTertiaryInverted
        imageWithTitleView?.titleFont = .semibold16

        imageWithTitleView?.spacingBetweenLabelAndIcon = 0.0
    }

    func applyIconStyle() {
        roundedBackgroundView?.shadowOpacity = 0.0
        roundedBackgroundView?.fillColor = .clear
        roundedBackgroundView?.highlightedFillColor = .clear
        roundedBackgroundView?.strokeColor = .clear
        roundedBackgroundView?.highlightedStrokeColor = .clear

        changesContentOpacityWhenHighlighted = true

        imageWithTitleView?.spacingBetweenLabelAndIcon = 0.0
    }

    func applyTitleIconStyle() {
        applyTitleStyle()

        imageWithTitleView?.spacingBetweenLabelAndIcon = 8.0
    }

    func applyCaptionTitleIconStyle() {
        roundedBackgroundView?.shadowOpacity = 0.0
        roundedBackgroundView?.strokeColor = .clear
        roundedBackgroundView?.highlightedStrokeColor = .clear
        roundedBackgroundView?.cornerRadius = 32
        roundedBackgroundView?.fillColor = .white12
        imageWithTitleView?.titleColor = .fgStaticWhite
        imageWithTitleView?.titleFont = .regular12
        contentInsets = .init(top: 0, left: 0, bottom: 0, right: 16)
        changesContentOpacityWhenHighlighted = true
    }

    func applyDestructiveStyle() {
        applyBaseSecondaryStyle()

        imageWithTitleView?.titleColor = .fgStaticWhite
        roundedBackgroundView?.fillColor = .bgStatusError
        roundedBackgroundView?.highlightedFillColor = .bgStatusError
    }

    func applyDisabledStyle() {
        applyBaseSecondaryStyle()

        imageWithTitleView?.titleColor = .fgDisabled
        roundedBackgroundView?.fillColor = .bgActionDisabled
        roundedBackgroundView?.highlightedFillColor = .bgActionDisabled
    }

    func applyBarButtonItemStyle() {
        applyBaseFillStyle()

        roundedBackgroundView?.fillColor = .fill12
        roundedBackgroundView?.highlightedFillColor = .fill12
        contentInsets = UIEdgeInsets(horizontal: 8, vertical: 8)
        roundedBackgroundView?.cornerRadius = 20

        changesContentOpacityWhenHighlighted = true
    }
}

extension RoundedButton.Style {
    static var transparent: RoundedButton.Style {
        .init(
            background: .init(
                shadow: .init(
                    shadowOpacity: 0,
                    shadowColor: nil,
                    shadowRadius: nil,
                    shadowOffset: nil
                ),
                fillColor: .black24,
                highlightedFillColor: .black24,
                rounding: .init(
                    radius: 24,
                    corners: .allCorners
                )
            ),
            title: .init(
                normalColor: .fgPrimaryInverted.withAlphaComponent(0.66),
                highlightedColor: .fgPrimaryInverted.withAlphaComponent(0.66),
                font: .semibold16
            )
        )
    }

    static var mainDark: RoundedButton.Style {
        .init(
            background: .init(
                shadow: .init(
                    shadowOpacity: 0,
                    shadowColor: nil,
                    shadowRadius: nil,
                    shadowOffset: nil
                ),
                fillColor: .bgActionTertiary,
                highlightedFillColor: .bgActionTertiaryHover,
                rounding: .init(
                    radius: 12,
                    corners: .allCorners
                )
            ),
            title: .init(
                normalColor: .fgPrimary,
                highlightedColor: .fgPrimary,
                font: .semibold16
            )
        )
    }

    static var white: RoundedButton.Style {
        .init(
            background: .init(
                shadow: .init(
                    shadowOpacity: 0,
                    shadowColor: nil,
                    shadowRadius: nil,
                    shadowOffset: nil
                ),
                fillColor: .bgActionPrimary,
                highlightedFillColor: .bgActionPrimaryHover,
                rounding: .init(
                    radius: 12,
                    corners: .allCorners
                )
            ),
            title: .init(
                normalColor: .fgPrimaryInverted,
                highlightedColor: .fgPrimaryInverted,
                font: .semibold16
            )
        )
    }
}
