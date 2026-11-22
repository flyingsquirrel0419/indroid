import UIKit

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    func application(_ application: UIApplication, didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        // Android boots as soon as the app opens: keep the screen awake for it.
        application.isIdleTimerDisabled = true
        let window = UIWindow(frame: UIScreen.main.bounds)
        window.rootViewController = NeoController()
        window.makeKeyAndVisible()
        self.window = window
        return true
    }
}
