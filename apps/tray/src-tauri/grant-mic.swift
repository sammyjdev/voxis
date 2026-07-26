// Bundled GrantMic.app, launched via `open` by the tray at first launch to
// raise the macOS microphone prompt. VOXIS is a menu-bar (accessory) app and
// the mic is opened by a spawned, bundle-less Python daemon - neither can
// present the TCC dialog (an in-process objc2 requestAccess also stayed a
// silent no-op). A short-lived Regular-policy Swift app launched through
// LaunchServices CAN; sharing VOXIS's bundle id + signing identity, the grant
// lands on VOXIS.app and unblocks the daemon's capture. Must run de-nested (the
// tray dittos it out of the bundle) - a nested .app is auto-denied the mic.
//
// Exits immediately unless status is notDetermined.
import AVFoundation
import AppKit

let app = NSApplication.shared
app.setActivationPolicy(.regular)

guard AVCaptureDevice.authorizationStatus(for: .audio) == .notDetermined else {
    exit(0)
}

AVCaptureDevice.requestAccess(for: .audio) { _ in exit(0) }
app.activate(ignoringOtherApps: true)
// pump the run loop so the prompt shows and the handler can fire; bounded so a
// dismissed prompt never leaves the helper resident
RunLoop.main.run(until: Date().addingTimeInterval(120))
