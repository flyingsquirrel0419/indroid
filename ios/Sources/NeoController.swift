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
        let stack = UIStackView(arrangedSubviews: [status, button, textView])
        stack.axis = .vertical
        stack.spacing = 8
        stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)
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
