import BigInt
import Foundation
import SubstrateSdk

// MARK: - Main Widget Tree

public enum ScaleWidget {
    case `nil`
    case string(String)
    case box(ScaleComponent<ScaleBoxProps>)
    case column(ScaleComponent<ScaleColumnProps>)
    case row(ScaleComponent<ScaleRowProps>)
    case spacer(ScaleComponent<ScaleVoidProps>)
    case text(ScaleComponent<ScaleTextProps>)
    case button(ScaleComponent<ScaleButtonProps>)
    case textField(ScaleComponent<ScaleTextFieldProps>)

    public static func decode(from hexString: String) throws -> ScaleWidget {
        let data = try Data(hexString: hexString)
        let decoder = try ScaleDecoder(data: data)
        return try ScaleWidget(scaleDecoder: decoder)
    }
}

extension ScaleWidget: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        let discriminant = try UInt8(scaleDecoder: scaleDecoder)
        switch discriminant {
        case 0:
            self = .nil
        case 1:
            self = try .string(String(scaleDecoder: scaleDecoder))
        case 2:
            self = try .box(ScaleComponent(scaleDecoder: scaleDecoder))
        case 3:
            self = try .column(ScaleComponent(scaleDecoder: scaleDecoder))
        case 4:
            self = try .row(ScaleComponent(scaleDecoder: scaleDecoder))
        case 5:
            self = try .spacer(ScaleComponent(scaleDecoder: scaleDecoder))
        case 6:
            self = try .text(ScaleComponent(scaleDecoder: scaleDecoder))
        case 7:
            self = try .button(ScaleComponent(scaleDecoder: scaleDecoder))
        case 8:
            self = try .textField(ScaleComponent(scaleDecoder: scaleDecoder))
        default:
            throw ScaleDecoderError.outOfBounds
        }
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case .nil:
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
        case let .string(value):
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            try value.encode(scaleEncoder: scaleEncoder)
        case let .box(component):
            try UInt8(2).encode(scaleEncoder: scaleEncoder)
            try component.encode(scaleEncoder: scaleEncoder)
        case let .column(component):
            try UInt8(3).encode(scaleEncoder: scaleEncoder)
            try component.encode(scaleEncoder: scaleEncoder)
        case let .row(component):
            try UInt8(4).encode(scaleEncoder: scaleEncoder)
            try component.encode(scaleEncoder: scaleEncoder)
        case let .spacer(component):
            try UInt8(5).encode(scaleEncoder: scaleEncoder)
            try component.encode(scaleEncoder: scaleEncoder)
        case let .text(component):
            try UInt8(6).encode(scaleEncoder: scaleEncoder)
            try component.encode(scaleEncoder: scaleEncoder)
        case let .button(component):
            try UInt8(7).encode(scaleEncoder: scaleEncoder)
            try component.encode(scaleEncoder: scaleEncoder)
        case let .textField(component):
            try UInt8(8).encode(scaleEncoder: scaleEncoder)
            try component.encode(scaleEncoder: scaleEncoder)
        }
    }
}

// MARK: - Generic Component

public struct ScaleComponent<Props: ScaleCodable> {
    public let modifiers: [ScaleModifier]
    public let props: Props
    public let children: [ScaleWidget]
}

extension ScaleComponent: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        modifiers = try [ScaleModifier](scaleDecoder: scaleDecoder)
        props = try Props(scaleDecoder: scaleDecoder)
        children = try [ScaleWidget](scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try modifiers.encode(scaleEncoder: scaleEncoder)
        try props.encode(scaleEncoder: scaleEncoder)
        try children.encode(scaleEncoder: scaleEncoder)
    }
}

// MARK: - Modifier

public enum ScaleModifier {
    case margin(ScaleDimensions)
    case padding(ScaleDimensions)
    case background(ScaleBackground)
    case border(ScaleBorderStyle)
    case height(BigUInt)
    case width(BigUInt)
    case minWidth(BigUInt)
    case minHeight(BigUInt)
    case fillWidth(Bool)
    case fillHeight(Bool)
}

extension ScaleModifier: ScaleCodable {
    // swiftlint:disable:next cyclomatic_complexity
    public init(scaleDecoder: any ScaleDecoding) throws {
        let discriminant = try UInt8(scaleDecoder: scaleDecoder)
        switch discriminant {
        case 0:
            self = try .margin(ScaleDimensions(scaleDecoder: scaleDecoder))
        case 1:
            self = try .padding(ScaleDimensions(scaleDecoder: scaleDecoder))
        case 2:
            self = try .background(ScaleBackground(scaleDecoder: scaleDecoder))
        case 3:
            self = try .border(ScaleBorderStyle(scaleDecoder: scaleDecoder))
        case 4:
            self = try .height(BigUInt(scaleDecoder: scaleDecoder))
        case 5:
            self = try .width(BigUInt(scaleDecoder: scaleDecoder))
        case 6:
            self = try .minWidth(BigUInt(scaleDecoder: scaleDecoder))
        case 7:
            self = try .minHeight(BigUInt(scaleDecoder: scaleDecoder))
        case 8:
            self = try .fillWidth(Bool(scaleDecoder: scaleDecoder))
        case 9:
            self = try .fillHeight(Bool(scaleDecoder: scaleDecoder))
        default:
            throw ScaleDecoderError.outOfBounds
        }
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .margin(dims):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try dims.encode(scaleEncoder: scaleEncoder)
        case let .padding(dims):
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
            try dims.encode(scaleEncoder: scaleEncoder)
        case let .background(background):
            try UInt8(2).encode(scaleEncoder: scaleEncoder)
            try background.encode(scaleEncoder: scaleEncoder)
        case let .border(border):
            try UInt8(3).encode(scaleEncoder: scaleEncoder)
            try border.encode(scaleEncoder: scaleEncoder)
        case let .height(size):
            try UInt8(4).encode(scaleEncoder: scaleEncoder)
            try size.encode(scaleEncoder: scaleEncoder)
        case let .width(size):
            try UInt8(5).encode(scaleEncoder: scaleEncoder)
            try size.encode(scaleEncoder: scaleEncoder)
        case let .minWidth(size):
            try UInt8(6).encode(scaleEncoder: scaleEncoder)
            try size.encode(scaleEncoder: scaleEncoder)
        case let .minHeight(size):
            try UInt8(7).encode(scaleEncoder: scaleEncoder)
            try size.encode(scaleEncoder: scaleEncoder)
        case let .fillWidth(value):
            try UInt8(8).encode(scaleEncoder: scaleEncoder)
            try value.encode(scaleEncoder: scaleEncoder)
        case let .fillHeight(value):
            try UInt8(9).encode(scaleEncoder: scaleEncoder)
            try value.encode(scaleEncoder: scaleEncoder)
        }
    }
}

// MARK: - Supporting Structs

public struct ScaleDimensions {
    public let horizontal: BigUInt
    public let vertical: BigUInt
    public let start: ScaleOption<BigUInt>
    public let end: ScaleOption<BigUInt>
}

extension ScaleDimensions: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        horizontal = try BigUInt(scaleDecoder: scaleDecoder)
        vertical = try BigUInt(scaleDecoder: scaleDecoder)
        start = try ScaleOption<BigUInt>(scaleDecoder: scaleDecoder)
        end = try ScaleOption<BigUInt>(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try horizontal.encode(scaleEncoder: scaleEncoder)
        try vertical.encode(scaleEncoder: scaleEncoder)
        try start.encode(scaleEncoder: scaleEncoder)
        try end.encode(scaleEncoder: scaleEncoder)
    }
}

public struct ScaleBackground {
    public let color: ScaleColorToken
    public let shape: ScaleOption<ScaleShape>
}

extension ScaleBackground: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        color = try ScaleColorToken(scaleDecoder: scaleDecoder)
        shape = try ScaleOption<ScaleShape>(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try color.encode(scaleEncoder: scaleEncoder)
        try shape.encode(scaleEncoder: scaleEncoder)
    }
}

public struct ScaleBorderStyle {
    public let width: BigUInt
    public let color: ScaleColorToken
    public let shape: ScaleOption<ScaleShape>
}

extension ScaleBorderStyle: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        width = try BigUInt(scaleDecoder: scaleDecoder)
        color = try ScaleColorToken(scaleDecoder: scaleDecoder)
        shape = try ScaleOption<ScaleShape>(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try width.encode(scaleEncoder: scaleEncoder)
        try color.encode(scaleEncoder: scaleEncoder)
        try shape.encode(scaleEncoder: scaleEncoder)
    }
}

// MARK: - Void Props (for Spacer)

public struct ScaleVoidProps {}

extension ScaleVoidProps: ScaleCodable {
    public init(scaleDecoder _: any ScaleDecoding) throws {}
    public func encode(scaleEncoder _: any ScaleEncoding) throws {}
}

// MARK: - Component Props

public struct ScaleBoxProps {
    public let contentAlignment: ScaleOption<ScaleContentAlignment>
}

extension ScaleBoxProps: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        contentAlignment = try ScaleOption<ScaleContentAlignment>(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try contentAlignment.encode(scaleEncoder: scaleEncoder)
    }
}

public struct ScaleColumnProps {
    public let horizontalAlignment: ScaleOption<ScaleHorizontalAlignment>
    public let verticalArrangement: ScaleOption<ScaleArrangement>
}

extension ScaleColumnProps: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        horizontalAlignment = try ScaleOption<ScaleHorizontalAlignment>(scaleDecoder: scaleDecoder)
        verticalArrangement = try ScaleOption<ScaleArrangement>(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try horizontalAlignment.encode(scaleEncoder: scaleEncoder)
        try verticalArrangement.encode(scaleEncoder: scaleEncoder)
    }
}

public struct ScaleRowProps {
    public let verticalAlignment: ScaleOption<ScaleVerticalAlignment>
    public let horizontalArrangement: ScaleOption<ScaleArrangement>
}

extension ScaleRowProps: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        verticalAlignment = try ScaleOption<ScaleVerticalAlignment>(scaleDecoder: scaleDecoder)
        horizontalArrangement = try ScaleOption<ScaleArrangement>(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try verticalAlignment.encode(scaleEncoder: scaleEncoder)
        try horizontalArrangement.encode(scaleEncoder: scaleEncoder)
    }
}

public struct ScaleTextProps {
    public let style: ScaleOption<ScaleTypographyStyle>
    public let color: ScaleOption<ScaleColorToken>
}

extension ScaleTextProps: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        style = try ScaleOption<ScaleTypographyStyle>(scaleDecoder: scaleDecoder)
        color = try ScaleOption<ScaleColorToken>(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try style.encode(scaleEncoder: scaleEncoder)
        try color.encode(scaleEncoder: scaleEncoder)
    }
}

public struct ScaleButtonProps {
    public let text: String
    public let variant: ScaleOption<ScaleButtonVariant>
    public let enabled: ScaleBoolOption
    public let loading: ScaleBoolOption
    public let clickAction: ScaleOption<String>
}

extension ScaleButtonProps: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        text = try String(scaleDecoder: scaleDecoder)
        variant = try ScaleOption<ScaleButtonVariant>(scaleDecoder: scaleDecoder)
        enabled = try ScaleBoolOption(scaleDecoder: scaleDecoder)
        loading = try ScaleBoolOption(scaleDecoder: scaleDecoder)
        clickAction = try ScaleOption<String>(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try text.encode(scaleEncoder: scaleEncoder)
        try variant.encode(scaleEncoder: scaleEncoder)
        try enabled.encode(scaleEncoder: scaleEncoder)
        try loading.encode(scaleEncoder: scaleEncoder)
        try clickAction.encode(scaleEncoder: scaleEncoder)
    }
}

public struct ScaleTextFieldProps {
    public let text: String
    public let placeholder: ScaleOption<String>
    public let label: ScaleOption<String>
    public let enabled: ScaleBoolOption
    public let valueChangeAction: ScaleOption<String>
}

extension ScaleTextFieldProps: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        text = try String(scaleDecoder: scaleDecoder)
        placeholder = try ScaleOption<String>(scaleDecoder: scaleDecoder)
        label = try ScaleOption<String>(scaleDecoder: scaleDecoder)
        enabled = try ScaleBoolOption(scaleDecoder: scaleDecoder)
        valueChangeAction = try ScaleOption<String>(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try text.encode(scaleEncoder: scaleEncoder)
        try placeholder.encode(scaleEncoder: scaleEncoder)
        try label.encode(scaleEncoder: scaleEncoder)
        try enabled.encode(scaleEncoder: scaleEncoder)
        try valueChangeAction.encode(scaleEncoder: scaleEncoder)
    }
}

// MARK: - Shape

public enum ScaleShape {
    case rounded(BigUInt)
    case circle
}

extension ScaleShape: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        let discriminant = try UInt8(scaleDecoder: scaleDecoder)
        switch discriminant {
        case 0:
            self = try .rounded(BigUInt(scaleDecoder: scaleDecoder))
        case 1:
            self = .circle
        default:
            throw ScaleDecoderError.outOfBounds
        }
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .rounded(radius):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            try radius.encode(scaleEncoder: scaleEncoder)
        case .circle:
            try UInt8(1).encode(scaleEncoder: scaleEncoder)
        }
    }
}

// MARK: - Design Token Enums

public enum ScaleColorToken: UInt8, ScaleCodable {
    case textPrimary = 0
    case textSecondary = 1
    case textTertiary = 2
    case backgroundPrimary = 3
    case backgroundSecondary = 4
    case backgroundTertiary = 5
    case success = 6
    case error = 7
    case warning = 8

    public init(scaleDecoder: any ScaleDecoding) throws {
        let raw = try UInt8(scaleDecoder: scaleDecoder)
        guard let value = ScaleColorToken(rawValue: raw) else {
            throw ScaleDecoderError.outOfBounds
        }
        self = value
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

public enum ScaleTypographyStyle: UInt8, ScaleCodable {
    case titleXL = 0
    case headline = 1
    case bodyM = 2
    case bodyS = 3
    case caption = 4

    public init(scaleDecoder: any ScaleDecoding) throws {
        let raw = try UInt8(scaleDecoder: scaleDecoder)
        guard let value = ScaleTypographyStyle(rawValue: raw) else {
            throw ScaleDecoderError.outOfBounds
        }
        self = value
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

public enum ScaleButtonVariant: UInt8, ScaleCodable {
    case primary = 0
    case secondary = 1
    case text = 2

    public init(scaleDecoder: any ScaleDecoding) throws {
        let raw = try UInt8(scaleDecoder: scaleDecoder)
        guard let value = ScaleButtonVariant(rawValue: raw) else {
            throw ScaleDecoderError.outOfBounds
        }
        self = value
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

public enum ScaleContentAlignment: UInt8, ScaleCodable {
    case topStart = 0
    case topCenter = 1
    case topEnd = 2
    case centerStart = 3
    case center = 4
    case centerEnd = 5
    case bottomStart = 6
    case bottomCenter = 7
    case bottomEnd = 8

    public init(scaleDecoder: any ScaleDecoding) throws {
        let raw = try UInt8(scaleDecoder: scaleDecoder)
        guard let value = ScaleContentAlignment(rawValue: raw) else {
            throw ScaleDecoderError.outOfBounds
        }
        self = value
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

public enum ScaleHorizontalAlignment: UInt8, ScaleCodable {
    case start = 0
    case center = 1
    case end = 2

    public init(scaleDecoder: any ScaleDecoding) throws {
        let raw = try UInt8(scaleDecoder: scaleDecoder)
        guard let value = ScaleHorizontalAlignment(rawValue: raw) else {
            throw ScaleDecoderError.outOfBounds
        }
        self = value
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

public enum ScaleVerticalAlignment: UInt8, ScaleCodable {
    case top = 0
    case center = 1
    case bottom = 2

    public init(scaleDecoder: any ScaleDecoding) throws {
        let raw = try UInt8(scaleDecoder: scaleDecoder)
        guard let value = ScaleVerticalAlignment(rawValue: raw) else {
            throw ScaleDecoderError.outOfBounds
        }
        self = value
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

public enum ScaleArrangement: UInt8, ScaleCodable {
    case start = 0
    case end = 1
    case center = 2
    case spaceBetween = 3
    case spaceAround = 4
    case spaceEvenly = 5

    public init(scaleDecoder: any ScaleDecoding) throws {
        let raw = try UInt8(scaleDecoder: scaleDecoder)
        guard let value = ScaleArrangement(rawValue: raw) else {
            throw ScaleDecoderError.outOfBounds
        }
        self = value
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}
