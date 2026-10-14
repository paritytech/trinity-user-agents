import DesignSystem
import PolkadotUI
import SnapKit
import UIKit

/// Scan panel without the in-panel search field: a camera above a button that opens full-screen
/// search. `EmbeddedQRScannerViewLayout` is a bare preview now, so the insets and the startup
/// placeholder live here.
final class ScanPanelPlainViewLayout: UIView {
    let grabber = DSPanelGrabberView()
    let searchButton = SearchContactFieldButton()

    /// Stands in for the camera while `AVCaptureSession` configures and starts, which takes
    /// roughly a second. It sits behind the preview, which fades in over it.
    private let placeholderView: UIView = {
        let view = UIView()
        view.backgroundColor = .bgSurfaceNested
        view.layer.cornerRadius = DSRadii.large
        view.layer.masksToBounds = true
        return view
    }()

    override init(frame: CGRect) {
        super.init(frame: frame)

        addSubview(grabber)
        addSubview(searchButton)

        grabber.snp.makeConstraints { make in
            make.top.leading.trailing.equalToSuperview()
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func setupScannerView(_ scannerView: UIView) {
        insertSubview(placeholderView, at: 0)
        insertSubview(scannerView, at: 1)

        scannerView.snp.makeConstraints { make in
            make.top.equalTo(grabber.snp.bottom).offset(DSSpacings.mediumIncreased)
            make.leading.trailing.equalToSuperview().inset(DSSpacings.mediumIncreased)
        }

        placeholderView.snp.makeConstraints { make in
            make.edges.equalTo(scannerView)
        }

        searchButton.snp.makeConstraints { make in
            make.top.equalTo(scannerView.snp.bottom).offset(DSSpacings.small)
            make.leading.trailing.equalToSuperview().inset(DSSpacings.mediumIncreased)
            make.bottom.equalToSuperview().inset(DSSpacings.small)
        }
    }
}
