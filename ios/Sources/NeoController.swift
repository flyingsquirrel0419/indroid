import UIKit

/// indroid: boots real Android 13 (arm64) as soon as the app opens. The first
/// launch downloads the system image from Google's public SDK repository and
/// installs it; every launch after that boots straight away. Shows the
/// console/logcat while Android starts.
final class NeoController: UIViewController {
    static let imageURL = URL(string: "https://dl.google.com/android/repository/sys-img/android/arm64-v8a-33_r02.zip")!
    private let fm = FileManager.default
    private let textView = UITextView()
    private let button = UIButton(type: .system)
    private let status = UILabel()
    private var log = ""
    private var pendingFlush = false
    private var booted = false
    private var timer: Timer?
    private let display = DisplayView()
    private let overlay = UIView()
    private let logButton = UIButton(type: .system)
    private var framePending = false
    private var latestFrame: (Int, Int, Data)?
    private let frameLock = NSLock()
    private var sawFrame = false

    private var base: URL { fm.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("neo", isDirectory: true) }
    private var rom: URL { base.appendingPathComponent("rom13", isDirectory: true) }
    private var state: URL { base.appendingPathComponent("state13", isDirectory: true) }
    private var romReady: Bool { fm.fileExists(atPath: rom.appendingPathComponent("system/bin/init").path) }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        textView.isEditable = false
        textView.font = .monospacedSystemFont(ofSize: 10, weight: .regular)
        textView.backgroundColor = .black
        textView.textColor = .green
        status.font = .preferredFont(forTextStyle: .footnote)
        status.numberOfLines = 0
        button.titleLabel?.font = .preferredFont(forTextStyle: .headline)
        button.addTarget(self, action: #selector(primary), for: .touchUpInside)
        display.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(display)
        overlay.backgroundColor = .systemBackground
        overlay.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(overlay)
        logButton.setTitle("Log", for: .normal)
        logButton.backgroundColor = UIColor.black.withAlphaComponent(0.4)
        logButton.setTitleColor(.white, for: .normal)
        logButton.layer.cornerRadius = 12
        logButton.contentEdgeInsets = UIEdgeInsets(top: 4, left: 10, bottom: 4, right: 10)
        logButton.isHidden = true
        logButton.addTarget(self, action: #selector(toggleLog), for: .touchUpInside)
        logButton.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(logButton)
        NSLayoutConstraint.activate([
            display.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            display.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            display.topAnchor.constraint(equalTo: view.topAnchor),
            display.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            overlay.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            overlay.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            overlay.topAnchor.constraint(equalTo: view.topAnchor),
            overlay.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            logButton.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -8),
            logButton.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 2),
        ])
        let stack = UIStackView(arrangedSubviews: [status, button, textView])
        stack.axis = .vertical
        stack.spacing = 8
        stack.translatesAutoresizingMaskIntoConstraints = false
        overlay.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor, constant: 12),
            stack.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -12),
            stack.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 8),
            stack.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor),
        ])
        refreshButton()
    }

    private var started = false
    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        guard !started else { return }
        started = true
        primary()
    }

    @objc private func toggleLog() { overlay.isHidden.toggle() }

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
                self.overlay.isHidden = true
                self.logButton.isHidden = false
                self.timer?.invalidate()
            }
        }
    }

    private func refreshButton() {
        if booted {
            button.setTitle("Running", for: .normal)
            button.isEnabled = false
        } else if romReady {
            button.setTitle("Boot Android", for: .normal)
            button.isEnabled = true
            status.text = "Android 13 system image installed."
        } else {
            button.setTitle("Download Android 13 (676 MB)", for: .normal)
            status.text = "The Android system image is downloaded from Google's public Android SDK repository and installed on this device. Nothing leaves your phone."
        }
    }

    @objc private func primary() {
        if romReady { boot() } else { download() }
    }

    private func append(_ s: String) {
        DispatchQueue.main.async {
            self.log += s
            if self.log.count > 400_000 { self.log = String(self.log.suffix(300_000)) }
            if !self.pendingFlush {
                self.pendingFlush = true
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) {
                    self.pendingFlush = false
                    self.textView.text = self.log
                    self.textView.scrollRangeToVisible(NSRange(location: (self.log as NSString).length - 1, length: 1))
                }
            }
        }
    }

    private func download() {
        button.isEnabled = false
        status.text = "Downloading…"
        let task = URLSession.shared.downloadTask(with: Self.imageURL) { [weak self] tmp, _, error in
            guard let self else { return }
            guard let tmp, error == nil else {
                DispatchQueue.main.async {
                    self.timer?.invalidate()
                    self.status.text = "Download failed: \(error?.localizedDescription ?? "?")"
                    self.button.setTitle("Retry download", for: .normal)
                    self.button.isEnabled = true
                }
                return
            }
            let zip = self.base.appendingPathComponent("image.zip")
            try? self.fm.createDirectory(at: self.base, withIntermediateDirectories: true)
            try? self.fm.removeItem(at: zip)
            try? self.fm.moveItem(at: tmp, to: zip)
            DispatchQueue.main.async { self.status.text = "Installing…" }
            DispatchQueue.global(qos: .userInitiated).async { self.install(zip) }
        }
        task.resume()
        timer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            guard let self, task.state == .running, task.countOfBytesExpectedToReceive > 0 else { return }
            let p = Double(task.countOfBytesReceived) / Double(task.countOfBytesExpectedToReceive)
            self.status.text = String(format: "Downloading… %.0f%%", p * 100)
        }
    }

    private func install(_ zip: URL) {
        let ctx = Unmanaged.passUnretained(self).toOpaque()
        let rc = indroid_neo_install(zip.path, rom.path, base.appendingPathComponent("work").path, { ctx, text, len in
            guard let ctx, let text else { return }
            let me = Unmanaged<NeoController>.fromOpaque(ctx).takeUnretainedValue()
            let s = String(decoding: UnsafeBufferPointer(start: text, count: len), as: UTF8.self)
            DispatchQueue.main.async { me.status.text = "Installing: \(s)" }
            me.append("[install] \(s)\n")
        }, ctx)
        try? fm.removeItem(at: zip)
        DispatchQueue.main.async {
            self.timer?.invalidate()
            self.button.isEnabled = true
            self.status.text = rc == 0 ? "Installed." : "Install failed (see log)."
            self.refreshButton()
            if rc == 0 { self.boot() }
        }
    }

    private func boot() {
        booted = true
        refreshButton()
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
        status.text = rc == 0 ? "Booting Android… (console and logcat below)" : "Boot failed."
        timer = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
            let n = indroid_neo_instructions()
            self?.status.text = String(format: "Booting Android… %.1f G guest instructions retired by exited threads", Double(n) / 1e9)
        }
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
