import SwiftUI

struct SettingsView: View {
    let model: AppModel
    // TODO: move the token to the Keychain once device pairing replaces the shared token.
    @AppStorage(AppModel.serverURLKey) private var serverURL = ""
    @AppStorage(AppModel.tokenKey) private var token = ""

    var body: some View {
        Form {
            Section("Sync server") {
                TextField("URL", text: $serverURL, prompt: Text("https://not.example.ts.net"))
                SecureField("Token", text: $token)
                Button("Connect") { model.connect(serverURL: serverURL, token: token) }
                    .keyboardShortcut(.defaultAction)
            }
            Section("This device") {
                LabeledContent("Device ID", value: model.deviceID)
                    .textSelection(.enabled)
            }
        }
        .formStyle(.grouped)
        .frame(width: 420)
    }
}
