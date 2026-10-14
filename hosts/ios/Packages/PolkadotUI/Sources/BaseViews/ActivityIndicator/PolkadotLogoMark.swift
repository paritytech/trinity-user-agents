import UIKit

/// Vector data for the six-petal Polkadot mark in a 278x266 box. Each petal has a fill outline
/// and a hairline stroke outline; strokes follow the original artwork, not the fill.
enum PolkadotLogoMark {
    static let viewBox = CGSize(width: 278, height: 266)

    struct Petal {
        let fill: UIBezierPath
        let stroke: UIBezierPath
    }

    static let petals: [Petal] = [
        Petal(fill: path(petal1Fill), stroke: path(petal1Stroke)),
        Petal(fill: path(petal2Fill), stroke: path(petal2Stroke)),
        Petal(fill: path(petal3Fill), stroke: path(petal3Stroke)),
        Petal(fill: path(petal4Fill), stroke: path(petal4Stroke)),
        Petal(fill: path(petal5Fill), stroke: path(petal5Stroke)),
        Petal(fill: path(petal6Fill), stroke: path(petal6Stroke))
    ]
}

private extension PolkadotLogoMark {
    enum PathOp {
        case move(CGFloat, CGFloat)
        case curve(CGFloat, CGFloat, CGFloat, CGFloat, CGFloat, CGFloat)
        case close
    }

    static func path(_ ops: [PathOp]) -> UIBezierPath {
        let path = UIBezierPath()
        for operation in ops {
            switch operation {
            case let .move(pointX, pointY):
                path.move(to: CGPoint(x: pointX, y: pointY))
            case let .curve(c1x, c1y, c2x, c2y, endX, endY):
                path.addCurve(
                    to: CGPoint(x: endX, y: endY),
                    controlPoint1: CGPoint(x: c1x, y: c1y),
                    controlPoint2: CGPoint(x: c2x, y: c2y)
                )
            case .close:
                path.close()
            }
        }
        return path
    }

    // Petal 1 — top-left
    static let petal1Fill: [PathOp] = [
        .move(60.9501, 73.4848),
        .curve(46.6807, 90.0984, 46.3045, 113.224, 60.1396, 125.091),
        .curve(73.9748, 136.987, 96.7536, 133.137, 111.052, 116.495),
        .curve(125.321, 99.8813, 125.697, 76.7554, 111.862, 64.8886),
        .curve(106.45, 60.2287, 99.6191, 58, 92.4989, 58),
        .curve(81.4423, 58, 69.6332, 63.3835, 60.9501, 73.4848),
        .close
    ]

    static let petal1Stroke: [PathOp] = [
        .move(92.499, 58.5),
        .curve(99.5144, 58.5, 106.225, 60.695, 111.536, 65.2676),
        .curve(125.099, 76.9018, 124.821, 99.6964, 110.673, 116.169),
        .curve(96.4956, 132.671, 74.029, 136.374, 60.4658, 124.712),
        .curve(46.9022, 113.079, 47.1809, 90.2833, 61.3291, 73.8105),
        .curve(69.9261, 63.8094, 81.6013, 58.5, 92.499, 58.5),
        .close
    ]

    // Petal 2 — bottom-left
    static let petal2Fill: [PathOp] = [
        .move(55.914, 153.161),
        .curve(44.9234, 166.206, 49.6929, 188.353, 66.5787, 202.613),
        .curve(83.4645, 216.873, 106.097, 217.881, 117.088, 204.836),
        .curve(128.079, 191.792, 123.309, 169.645, 106.423, 155.385),
        .curve(97.4767, 147.825, 86.9305, 144, 77.3619, 144),
        .curve(68.8598, 144, 61.0982, 147.024, 55.9436, 153.161),
        .close
    ]

    static let petal2Stroke: [PathOp] = petal2Fill

    // Petal 3 — bottom-center
    static let petal3Fill: [PathOp] = [
        .move(148.804, 197.346),
        .curve(129.699, 203.433, 116.535, 215.838, 119.39, 225.04),
        .curve(122.274, 234.242, 140.094, 236.781, 159.198, 230.665),
        .curve(178.303, 224.578, 191.467, 212.174, 188.612, 202.972),
        .curve(186.784, 197.173, 179.017, 194, 168.736, 194),
        .curve(162.711, 194, 155.857, 195.067, 148.804, 197.317),
        .close
    ]

    static let petal3Stroke: [PathOp] = petal3Fill

    // Petal 4 — top-center
    static let petal4Fill: [PathOp] = [
        .move(117.739, 41.9791),
        .curve(113.729, 53.6075, 126.458, 68.6252, 146.221, 75.5497),
        .curve(165.983, 82.4741, 185.252, 78.6467, 189.263, 67.0182),
        .curve(193.273, 55.3898, 180.544, 40.3721, 160.781, 33.4476),
        .curve(154.126, 31.1103, 147.5, 30, 141.513, 30),
        .curve(129.742, 30, 120.384, 34.2657, 117.739, 41.9791),
        .close
    ]

    static let petal4Stroke: [PathOp] = [
        .move(141.513, 30.5),
        .curve(147.439, 30.5, 154.01, 31.5991, 160.615, 33.9189),
        .curve(170.428, 37.3579, 178.471, 42.7975, 183.556, 48.792),
        .curve(188.649, 54.797, 190.714, 61.2764, 188.79, 66.8555),
        .curve(186.866, 72.4328, 181.251, 76.2238, 173.552, 77.751),
        .curve(165.865, 79.2756, 156.198, 78.5161, 146.386, 75.0781),
        .curve(136.574, 71.6401, 128.531, 66.1997, 123.446, 60.2051),
        .curve(118.512, 54.3879, 116.42, 48.1258, 118.043, 42.668),
        .curve(120.749, 34.7425, 129.806, 30.5, 141.513, 30.5),
        .close
    ]

    // Petal 5 — right-upper
    static let petal5Fill: [PathOp] = [
        .move(201.667, 61.2706),
        .curve(195.74, 63.6457, 197.154, 80.9326, 204.796, 99.813),
        .curve(212.438, 118.723, 223.42, 132.102, 229.348, 129.727),
        .curve(235.245, 127.352, 233.861, 110.095, 226.219, 91.1845),
        .curve(219.178, 73.7472, 209.279, 61, 203.141, 61),
        .curve(202.63, 61, 202.148, 61.0902, 201.667, 61.2706),
        .close
    ]

    static let petal5Stroke: [PathOp] = [
        .move(203.142, 61.5),
        .curve(204.513, 61.5001, 206.174, 62.2193, 208.041, 63.6602),
        .curve(209.897, 65.0927, 211.9, 67.1951, 213.945, 69.8574),
        .curve(218.034, 75.1805, 222.248, 82.6857, 225.755, 91.3721),
        .curve(229.562, 100.793, 231.803, 109.784, 232.362, 116.696),
        .curve(232.642, 120.155, 232.498, 123.065, 231.938, 125.237),
        .curve(231.409, 127.285, 230.537, 128.586, 229.394, 129.158),
        .curve(227.895, 129.77, 226.273, 129.469, 224.347, 128.288),
        .curve(222.43, 127.113, 220.304, 125.118, 218.102, 122.435),
        .curve(213.7, 117.072, 209.067, 109.047, 205.26, 99.626),
        .curve(201.313, 89.6591, 199.187, 81.0111, 198.642, 74.3115),
        .curve(198.36, 70.8507, 198.502, 67.9389, 199.064, 65.7646),
        .curve(199.628, 63.585, 200.582, 62.2484, 201.842, 61.7383),
        .curve(202.27, 61.5792, 202.692, 61.5, 203.142, 61.5),
        .close
    ]

    // Petal 6 — right-lower
    static let petal6Fill: [PathOp] = [
        .move(206.531, 167.111),
        .curve(198.402, 184.647, 195.586, 200.624, 200.26, 202.762),
        .curve(204.934, 204.9, 215.327, 192.419, 223.456, 174.882),
        .curve(231.614, 157.346, 234.401, 141.369, 229.756, 139.231),
        .curve(229.408, 139.058, 229.03, 139, 228.595, 139),
        .curve(223.514, 139, 214.05, 150.903, 206.531, 167.111),
        .close
    ]

    static let petal6Stroke: [PathOp] = [
        .move(228.595, 139.5),
        .curve(228.987, 139.5, 229.279, 139.552, 229.533, 139.679),
        .curve(230.461, 140.106, 231.124, 141.269, 231.383, 143.301),
        .curve(231.638, 145.302, 231.479, 147.992, 230.915, 151.195),
        .curve(229.789, 157.595, 227.069, 165.931, 223.003, 174.671),
        .curve(218.951, 183.412, 214.342, 190.877, 210.178, 195.88),
        .curve(208.093, 198.384, 206.139, 200.25, 204.44, 201.354),
        .curve(202.716, 202.476, 201.391, 202.729, 200.468, 202.307),
        .curve(199.547, 201.885, 198.881, 200.722, 198.62, 198.691),
        .curve(198.363, 196.69, 198.522, 194, 199.086, 190.797),
        .curve(200.212, 184.397, 202.933, 176.062, 206.984, 167.321),
        .curve(210.732, 159.244, 214.958, 152.252, 218.87, 147.287),
        .curve(220.827, 144.804, 222.694, 142.841, 224.373, 141.504),
        .curve(226.069, 140.153, 227.5, 139.5, 228.595, 139.5),
        .close
    ]
}
