import UIKit

/// indroid: Android 13 (arm64) starts the moment the app opens. The first launch
/// downloads the system image from Google's public SDK repository and installs it;
/// every launch after that boots straight away. There is nothing to tap: the
/// launch screen is followed by a quiet status view that matches it (no flash),
/// then by Android's own display.
final class NeoController: UIViewController {
    static let imageURL = URL(string: "https://dl.google.com/android/repository/sys-img/android/arm64-v8a-33_r02.zip")!
    private let fm = FileManager.default
    private var log = ""
    private var pendingFlush = false
    private var timer: Timer?
    private let display = DisplayView()
    private let splash = UIView()
    private let spinner = UIActivityIndicatorView(style: .medium)
    private let status = UILabel()
    private let retryButton = UIButton(type: .system)
    private let logButton = UIButton(type: .system)
    private var framePending = false
    private var latestFrame: (Int, Int, Data)?
    private let frameLock = NSLock()
    private var sawFrame = false
    private weak var logController: LogController?

    private var base: URL { fm.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("neo", isDirectory: true) }
    private var rom: URL { base.appendingPathComponent("rom13", isDirectory: true) }
    private var state: URL { base.appendingPathComponent("state13", isDirectory: true) }
    private var romReady: Bool { fm.fileExists(atPath: rom.appendingPathComponent("system/bin/init").path) }

    // Android's screen is portrait and fills the display: hide system chrome once it shows.
    override var prefersStatusBarHidden: Bool { sawFrame }
    override var prefersHomeIndicatorAutoHidden: Bool { sawFrame }
    override var supportedInterfaceOrientations: UIInterfaceOrientationMask { .portrait }

    override func viewDidLoad() {
        super.viewDidLoad()
        // Same color as the system launch screen in light and dark appearance, so
        // nothing flashes between launch and the first screen.
        view.backgroundColor = .systemBackground
        display.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(display)

        splash.backgroundColor = .systemBackground
        splash.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(splash)

        spinner.hidesWhenStopped = true
        spinner.startAnimating()
        status.font = .preferredFont(forTextStyle: .footnote)
        status.adjustsFontForContentSizeCategory = true
        status.textColor = .secondaryLabel
        status.textAlignment = .center
        status.numberOfLines = 0
        status.accessibilityTraits = .updatesFrequently

        var retry = UIButton.Configuration.filled()
        retry.title = "Try Again"
        retry.cornerStyle = .capsule
        retryButton.configuration = retry
        retryButton.isHidden = true
        retryButton.addTarget(self, action: #selector(retryTapped), for: .touchUpInside)

        var logConfig = UIButton.Configuration.plain()
        logConfig.title = "Show Log"
        logButton.configuration = logConfig
        logButton.isHidden = true
        logButton.addTarget(self, action: #selector(showLog), for: .touchUpInside)

        let stack = UIStackView(arrangedSubviews: [spinner, status, retryButton, logButton])
        stack.axis = .vertical
        stack.alignment = .center
        stack.spacing = 12
        stack.translatesAutoresizingMaskIntoConstraints = false
        splash.addSubview(stack)

        NSLayoutConstraint.activate([
            display.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            display.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            display.topAnchor.constraint(equalTo: view.topAnchor),
            display.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            splash.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            splash.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            splash.topAnchor.constraint(equalTo: view.topAnchor),
            splash.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            stack.centerXAnchor.constraint(equalTo: splash.centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: splash.centerYAnchor),
            stack.leadingAnchor.constraint(greaterThanOrEqualTo: splash.layoutMarginsGuide.leadingAnchor, constant: 16),
            stack.trailingAnchor.constraint(lessThanOrEqualTo: splash.layoutMarginsGuide.trailingAnchor, constant: -16),
            retryButton.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
            logButton.heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
        ])
    }

    private var started = false
    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        guard !started else { return }
        started = true
        start()
    }

    private func start() {
        retryButton.isHidden = true
        logButton.isHidden = true
        spinner.startAnimating()
        if romReady { boot() } else { download() }
    }

    @objc private func retryTapped() { start() }

    /// Shows the console/logcat. It is a diagnostic, so it is not part of the normal flow.
    @objc private func showLog() {
        let vc = LogController(text: log)
        logController = vc
        present(UINavigationController(rootViewController: vc), animated: true)
    }

    private func fail(_ message: String) {
        timer?.invalidate()
        spinner.stopAnimating()
        status.text = message
        retryButton.isHidden = false
        logButton.isHidden = false
    }

    /// Called on a renderer thread for every composed frame. Copies the pixels
    /// and keeps only the newest one so a slow main thread never builds a backlog.
    fileprivate func deliver(width: Int, height: Int, rgba: UnsafePointer<UInt8>) {
        let data = Data(bytes: rgba, count: width * height * 4)
        frameLock.lock()
        latestFrame = (width, height, data)
        let schedule = !framePending
        framePending = true
        frameLock.unlock()
        guard schedule else { return }
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.frameLock.lock()
            let frame = self.latestFrame
            self.latestFrame = nil
            self.framePending = false
            self.frameLock.unlock()
            guard let (w, h, data) = frame, let provider = CGDataProvider(data: data as CFData),
                  let image = CGImage(width: w, height: h, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: w * 4,
                                      space: CGColorSpaceCreateDeviceRGB(),
                                      bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
                                      provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent) else { return }
            self.display.show(image)
            if !self.sawFrame {
                self.sawFrame = true
                self.timer?.invalidate()
                UIView.animate(withDuration: 0.25, animations: { self.splash.alpha = 0 }) { _ in self.splash.isHidden = true }
                self.setNeedsStatusBarAppearanceUpdate()
                self.setNeedsUpdateOfHomeIndicatorAutoHidden()
            }
        }
    }

    private func append(_ s: String) {
        DispatchQueue.main.async {
            self.log += s
            if self.log.count > 400_000 { self.log = String(self.log.suffix(300_000)) }
            if !self.pendingFlush {
                self.pendingFlush = true
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) {
                    self.pendingFlush = false
                    self.logController?.update(self.log)
                }
            }
        }
    }

    private func download() {
        status.text = "Preparing Android 13 for first launch…"
        let task = URLSession.shared.downloadTask(with: Self.imageURL) { [weak self] tmp, _, error in
            guard let self else { return }
            guard let tmp, error == nil else {
                DispatchQueue.main.async {
                    self.fail("Couldn’t download Android. Check your connection and try again.")
                    self.append("[download] \(error?.localizedDescription ?? "unknown error")\n")
                }
                return
            }
            let zip = self.base.appendingPathComponent("image.zip")
            try? self.fm.createDirectory(at: self.base, withIntermediateDirectories: true)
            try? self.fm.removeItem(at: zip)
            try? self.fm.moveItem(at: tmp, to: zip)
            DispatchQueue.main.async {
                self.timer?.invalidate()
                self.status.text = "Installing Android…"
            }
            DispatchQueue.global(qos: .userInitiated).async { self.install(zip) }
        }
        task.resume()
        timer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            guard let self, task.state == .running, task.countOfBytesExpectedToReceive > 0 else { return }
            let p = Double(task.countOfBytesReceived) / Double(task.countOfBytesExpectedToReceive)
            self.status.text = String(format: "Downloading Android 13 · %.0f%%", p * 100)
        }
    }

    private func install(_ zip: URL) {
        let ctx = Unmanaged.passUnretained(self).toOpaque()
        let rc = indroid_neo_install(zip.path, rom.path, base.appendingPathComponent("work").path, { ctx, text, len in
            guard let ctx, let text else { return }
            let me = Unmanaged<NeoController>.fromOpaque(ctx).takeUnretainedValue()
            let s = String(decoding: UnsafeBufferPointer(start: text, count: len), as: UTF8.self)
            me.append("[install] \(s)\n")
        }, ctx)
        try? fm.removeItem(at: zip)
        DispatchQueue.main.async {
            if rc == 0 { self.boot() } else { self.fail("Couldn’t install Android.") }
        }
    }

    private func boot() {
        status.text = "Starting Android…"
        let overlay = Bundle.main.url(forResource: "rom-overlay", withExtension: nil)!
        let overlays = [overlay.appendingPathComponent("emu13").path, overlay.appendingPathComponent("common").path].joined(separator: ":")
        try? fm.createDirectory(at: state, withIntermediateDirectories: true)
        let ctx = Unmanaged.passUnretained(self).toOpaque()
        indroid_neo_set_frame_callback({ ctx, w, h, rgba in
            guard let ctx, let rgba else { return }
            Unmanaged<NeoController>.fromOpaque(ctx).takeUnretainedValue().deliver(width: Int(w), height: Int(h), rgba: rgba)
        }, ctx)
        // GPU: the gfxstream renderer ships inside the app bundle (Frameworks/).
        if let gpu = Bundle.main.privateFrameworksURL?.appendingPathComponent("libgfxstream_backend.dylib"),
           fm.fileExists(atPath: gpu.path) {
            setenv("INDROID_GFXSTREAM", gpu.path, 1)
            // iOS has no system Vulkan loader: gfxstream talks to the bundled MoltenVK directly.
            if let mvk = Bundle.main.privateFrameworksURL?.appendingPathComponent("MoltenVK.framework/MoltenVK") {
                setenv("ANDROID_EMU_VK_LOADER_PATH", mvk.path, 1)
                setenv("ANDROID_EMU_VK_ICD", "moltenvk", 1)
            }
        } else {
            append("[gpu] libgfxstream_backend.dylib is not bundled in this build: Android will boot without a display.\n")
        }
        let rc = indroid_neo_boot(rom.path, state.path, overlays, nil, { ctx, text, len in
            guard let ctx, let text else { return }
            let me = Unmanaged<NeoController>.fromOpaque(ctx).takeUnretainedValue()
            me.append(String(decoding: UnsafeBufferPointer(start: text, count: len), as: UTF8.self))
        }, ctx)
        guard rc == 0 else { fail("Android couldn’t start."); return }
        // Boot takes a couple of minutes on the interpreter: say how far along it is,
        // and offer the log if no picture appears after a while.
        let began = Date()
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            guard let self, !self.sawFrame else { return }
            let seconds = Int(Date().timeIntervalSince(began))
            self.status.text = "Starting Android… \(seconds / 60):" + String(format: "%02d", seconds % 60)
            if seconds > 240 { self.logButton.isHidden = false }
        }
    }
}

/// Console and logcat, for diagnosing a boot that doesn't reach the display.
final class LogController: UIViewController {
    private let textView = UITextView()
    init(text: String) {
        super.init(nibName: nil, bundle: nil)
        textView.text = text
        title = "Log"
    }
    required init?(coder: NSCoder) { fatalError() }
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        textView.isEditable = false
        textView.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        textView.alwaysBounceVertical = true
        textView.frame = view.bounds
        textView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(textView)
        navigationItem.rightBarButtonItem = UIBarButtonItem(systemItem: .done, primaryAction: UIAction { [weak self] _ in
            self?.dismiss(animated: true)
        })
    }
    func update(_ text: String) {
        textView.text = text
        textView.scrollRangeToVisible(NSRange(location: (text as NSString).length, length: 0))
    }
}

/// Shows Android's composed frames (1080x2340 RGBA) and turns UIKit touches into
/// touchscreen events in the guest's pixel space.
final class DisplayView: UIView {
    static let guestWidth: CGFloat = 1080
    static let guestHeight: CGFloat = 2340
    private var slots: [UITouch: Int] = [:]
    private var nextTrackingID: Int32 = 1

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .black
        isMultipleTouchEnabled = true
        layer.contentsGravity = .resizeAspect
        layer.magnificationFilter = .linear
    }
    required init?(coder: NSCoder) { fatalError() }

    func show(_ image: CGImage) { layer.contents = image }

    /// Where the guest screen sits inside the view (aspect fit).
    private var screenRect: CGRect {
        let scale = min(bounds.width / Self.guestWidth, bounds.height / Self.guestHeight)
        let size = CGSize(width: Self.guestWidth * scale, height: Self.guestHeight * scale)
        return CGRect(x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2, width: size.width, height: size.height)
    }
    private func guestPoint(_ t: UITouch) -> (Int32, Int32) {
        let r = screenRect, p = t.location(in: self)
        let x = max(0, min(1, (p.x - r.minX) / r.width)) * (Self.guestWidth - 1)
        let y = max(0, min(1, (p.y - r.minY) / r.height)) * (Self.guestHeight - 1)
        return (Int32(x), Int32(y))
    }
    private func send(_ touches: Set<UITouch>, begin: Bool = false) {
        for t in touches {
            guard let slot = slots[t] else { continue }
            let (x, y) = guestPoint(t)
            indroid_neo_touch(Int32(slot), slotIDs[slot], x, y)
        }
    }
    private var slotIDs = [Int32](repeating: 0, count: 10)
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        for t in touches {
            guard let slot = (0..<10).first(where: { !slots.values.contains($0) }) else { continue }
            slots[t] = slot
            slotIDs[slot] = nextTrackingID
            nextTrackingID = nextTrackingID == Int32.max ? 1 : nextTrackingID + 1
        }
        send(touches)
    }
    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) { send(touches) }
    private func lift(_ touches: Set<UITouch>) {
        for t in touches {
            if let slot = slots.removeValue(forKey: t) { indroid_neo_touch(Int32(slot), -1, 0, 0) }
        }
    }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) { send(touches); lift(touches) }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) { lift(touches) }
}
