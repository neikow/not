import SwiftUI

@main
struct NotMacApp: App {
    @State private var model = AppModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        // A single `Window` (not `WindowGroup`) so `not://` URLs reuse it instead of opening new windows.
        Window("not", id: "main") {
            ContentView(model: model)
                .onOpenURL { model.handle(url: $0) }
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { model.syncNow() }
        }

        Settings {
            SettingsView(model: model)
        }
    }
}
