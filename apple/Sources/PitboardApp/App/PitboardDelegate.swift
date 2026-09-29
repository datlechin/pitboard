import AppKit
import Foundation

/// The app's delegate, and the owner of its model, so the model exists before the first link
/// from outside can arrive: a link that launches the app arrives before any view appears.
///
/// AppKit hands it every URL the app is asked to open, of the scheme the Info.plist claims,
/// at launch and while running alike. Every scene is kept from handling outside events
/// (`PitboardScenes`), so this is the only receiver and no scene opens for a link. It opens
/// nothing either: a pitboard link goes to the account picker, and anything else is ignored.
@MainActor
public final class PitboardDelegate: NSObject, NSApplicationDelegate {
    public let model: AppModel
    /// The Services provider, kept here as well as by AppKit.
    private var service: LinkService?

    public override init() {
        model = AppModel(dependencies: .forLaunch())
        super.init()
    }

    /// For a test, with a model of its own.
    init(model: AppModel) {
        self.model = model
        super.init()
    }

    /// Registers the Service provider, which is where AppKit says to: requests can arrive at
    /// once, the one that launched the app included.
    public func applicationDidFinishLaunching(_ notification: Notification) {
        let service = LinkService(links: model.links)
        self.service = service
        NSApp.servicesProvider = service
        if model.registersServices { updateServicesOncePerVersion() }
    }

    public func application(_ application: NSApplication, open urls: [URL]) {
        receive(urls)
    }

    /// Hands each pitboard link to the account picker. The picker ignores any other scheme.
    func receive(_ urls: [URL]) {
        for url in urls { model.links.receive(url) }
    }

    /// Asks macOS to read the app's Services again once per version, so **Open in pitboard**
    /// is offered after an install or an update without logging out. Never in a fixture.
    private func updateServicesOncePerVersion() {
        let info = Bundle.main.infoDictionary ?? [:]
        let version = [info["CFBundleShortVersionString"], info["CFBundleVersion"]]
            .compactMap { $0 as? String }.joined(separator: " ")
        guard model.defaults.string(forKey: DefaultsKey.servicesVersion) != version else {
            return
        }
        NSUpdateDynamicServices()
        model.defaults.set(version, forKey: DefaultsKey.servicesVersion)
    }
}
