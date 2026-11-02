import AppKit
import SwiftUI

@main
struct NotMacApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @Environment(\.scenePhase) private var scenePhase
    @AppStorage(MenuBarPanel.visibleKey) private var showsMenuBarExtra = true

    var body: some Scene {
        // A single `Window` (not `WindowGroup`): reopening it never stacks up copies.
        Window("not", id: MainWindow.id) {
            ContentView(model: appDelegate.model)
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { appDelegate.model.syncNow() }
        }

        MenuBarExtra(isInserted: $showsMenuBarExtra) {
            MenuBarPanel(model: appDelegate.model)
        } label: {
            Image(systemName: "checkmark.square")
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView(model: appDelegate.model)
        }
    }
}

enum MainWindow {
    static let id = "main"
}

/// Owns the model, so `not://` URLs work without a window, and keeps the app running (menu bar extra,
/// live sync) after the window is closed.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let model = AppModel()

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }

    /// Handled in place: quick add from scripts or Raycast shouldn't bring up the window.
    func application(_ application: NSApplication, open urls: [URL]) {
        urls.forEach(model.handle(url:))
    }
}
