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
        CommandGroup(after: .sidebar) {
            Divider()
            Button("Inbox") { model.showList(model.inboxID) }.keyboardShortcut("1")
            Button("Today") { model.showList(model.todayID) }.keyboardShortcut("2")
            Button("Upcoming") { model.showList(model.upcomingID) }.keyboardShortcut("3")
            Button("Someday") { model.showList(model.somedayID) }.keyboardShortcut("4")
            Button("Next List") { model.stepList(forward: true) }
                .keyboardShortcut(.downArrow, modifiers: [.command, .option])
            Button("Previous List") { model.stepList(forward: false) }
                .keyboardShortcut(.upArrow, modifiers: [.command, .option])
            Divider()
        }
        CommandMenu("Task") {
            Button(model.selectedTask?.completedAt == nil ? "Complete" : "Mark as Not Completed") {
                model.toggleSelected()
            }
            .keyboardShortcut("k")
            .disabled(model.selectedTask == nil)
            Button(model.showsInspector ? "Hide Info" : "Show Info") { model.showsInspector.toggle() }
                .keyboardShortcut("i")
            Divider()
            Button("Process Inbox…") { model.processingInbox = true }
                .keyboardShortcut("p", modifiers: [.command, .shift])
            Divider()
            Button("Add Subtask") { model.beginSubtask() }
                .keyboardShortcut("n", modifiers: [.command, .option])
                .disabled(model.selectedTask == nil || model.isDateView)
            Button("Indent") { model.indentSelected() }
                .keyboardShortcut("]")
                .disabled(model.selectedTask == nil || model.isDateView)
            Button("Outdent") { model.outdentSelected() }
                .keyboardShortcut("[")
                .disabled(model.selectedTask?.parentId == nil || model.isDateView)
        }
    }

    private var newItemTitle: String {
        switch model.selection {
        case .habits: "New Habit"
        case .notes: "New Note"
        case .workouts: "New Workout"
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
