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
            if phase == .active {
                appDelegate.model.refreshToday()
                appDelegate.model.syncNow()
            }
        }
        .commands { TaskCommands(model: appDelegate.model) }

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

private struct TaskCommands: Commands {
    let model: AppModel

    var body: some Commands {
        CommandGroup(replacing: .undoRedo) {
            Button("Undo") { undoOrRedo(redo: false) }
                .keyboardShortcut("z")
            Button("Redo") { undoOrRedo(redo: true) }
                .keyboardShortcut("z", modifiers: [.command, .shift])
        }
        CommandGroup(replacing: .newItem) {
            Button(newItemTitle) { model.newItem() }
                .keyboardShortcut("n")
        }
        CommandMenu("Task") {
            Button(model.selectedTask?.completedAt == nil ? "Complete" : "Mark as Not Completed") {
                model.toggleSelected()
            }
            .keyboardShortcut("k")
            .disabled(model.selectedTask == nil)
            Button(model.showsInspector ? "Hide Info" : "Show Info") { model.showsInspector.toggle() }
                .keyboardShortcut("i")
        }
    }

    private var newItemTitle: String {
        switch model.selection {
        case .habits: "New Habit"
        case .notes: "New Note"
        default: "New Task"
        }
    }

    /// Text being edited gets ⌘Z first; otherwise it undoes the last change to tasks and lists.
    private func undoOrRedo(redo: Bool) {
        if let editor = NSApp.keyWindow?.firstResponder as? NSTextView, let undo = editor.undoManager,
           redo ? undo.canRedo : undo.canUndo {
            redo ? undo.redo() : undo.undo()
        } else {
            redo ? model.redo() : model.undo()
        }
    }
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
