import SwiftUI
import WebKit

@main
@MainActor
struct NeptunePortableApp: App {
    var body: some Scene {
        WindowGroup {
            PortableSceneView()
        }
    }
}

private enum SceneStatus: Sendable {
    case loading
    case ready
    case failed(String)
}

@MainActor
private struct PortableSceneView: View {
    @State private var status = SceneStatus.loading
    @State private var attempt = 0

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            SceneWebView { status = $0 }
                .id(attempt)

            switch status {
            case .loading:
                ProgressView("Loading scene…")
                    .padding()
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
            case .ready:
                EmptyView()
            case .failed(let message):
                VStack(spacing: 16) {
                    Text(message)
                        .multilineTextAlignment(.center)
                    Button("Reload Scene") {
                        status = .loading
                        attempt += 1
                    }
                    .buttonStyle(.borderedProminent)
                }
                .padding()
                .frame(maxWidth: 360)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
                .padding()
            }
        }
    }
}

@MainActor
private struct SceneWebView: UIViewRepresentable {
    let report: (SceneStatus) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(report: report)
    }

    func makeUIView(context: Context) -> WKWebView {
        let coordinator = context.coordinator
        let configuration = WKWebViewConfiguration()
        configuration.allowsInlineMediaPlayback = true
        configuration.mediaTypesRequiringUserActionForPlayback = .all
        configuration.websiteDataStore = .nonPersistent()
        configuration.userContentController.addUserScript(
            WKUserScript(source: Self.fitViewport, injectionTime: .atDocumentEnd, forMainFrameOnly: true)
        )

        let entry = Bundle.main.url(forResource: "index", withExtension: "html", subdirectory: "Web")
        var startupError: String?
        if let entry {
            do {
                try LanTransport.install(in: configuration, entry: entry)
            } catch {
                startupError = error.localizedDescription
            }
        } else {
            startupError = "The bundled scene is missing."
        }

        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = coordinator
        view.isOpaque = false
        view.backgroundColor = .black
        view.scrollView.backgroundColor = .black
        view.scrollView.contentInsetAdjustmentBehavior = .never
        view.scrollView.bounces = false
        if let startupError {
            coordinator.publish(.failed(startupError))
        } else if let entry {
            coordinator.load(entry, in: view)
        }
        return view
    }

    func updateUIView(_ uiView: WKWebView, context: Context) {}

    static func dismantleUIView(_ uiView: WKWebView, coordinator: Coordinator) {
        coordinator.active = false
        uiView.navigationDelegate = nil
        uiView.stopLoading()
    }

    @MainActor
    final class Coordinator: NSObject, WKNavigationDelegate {
        private let report: (SceneStatus) -> Void
        var active = true
        private var entry: URL?

        init(report: @escaping (SceneStatus) -> Void) {
            self.report = report
        }

        func publish(_ status: SceneStatus) {
            // makeUIView can report a startup error before SwiftUI finishes building the view.
            DispatchQueue.main.async { [weak self] in
                guard let self, self.active else { return }
                self.report(status)
            }
        }

        func load(_ entry: URL, in view: WKWebView) {
            self.entry = entry.resolvingSymlinksInPath().standardizedFileURL
            // LAN requests use the native transport. A page cannot fetch remote
            // resources directly, even if a runtime asset contains an external URL.
            WKContentRuleListStore.default().compileContentRuleList(
                forIdentifier: "NeptunePortableHost.BlockRemote.v1",
                encodedContentRuleList: """
                [
                  {"trigger":{"url-filter":"^https?://"},"action":{"type":"block"}},
                  {"trigger":{"url-filter":"^wss?://"},"action":{"type":"block"}}
                ]
                """
            ) { [weak self, weak view] rules, _ in
                DispatchQueue.main.async {
                    guard let self, self.active, let view else { return }
                    guard let rules else {
                        self.publish(.failed("The scene’s network restrictions could not be loaded."))
                        return
                    }
                    view.configuration.userContentController.add(rules)
                    view.loadFileURL(entry, allowingReadAccessTo: entry.deletingLastPathComponent())
                }
            }
        }

        private func isEntry(_ url: URL?) -> Bool {
            guard let url, url.isFileURL, url.host == nil || url.host == "", url.query == nil else {
                return false
            }
            return url.resolvingSymlinksInPath().standardizedFileURL.path == entry?.path
        }

        func webView(
            _ webView: WKWebView,
            decidePolicyFor navigationAction: WKNavigationAction,
            decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
        ) {
            let allowed = navigationAction.targetFrame?.isMainFrame == true
                && isEntry(navigationAction.request.url)
            decisionHandler(allowed ? .allow : .cancel)
        }

        func webView(
            _ webView: WKWebView,
            decidePolicyFor navigationResponse: WKNavigationResponse,
            decisionHandler: @escaping (WKNavigationResponsePolicy) -> Void
        ) {
            let allowed = navigationResponse.isForMainFrame && isEntry(navigationResponse.response.url)
            decisionHandler(allowed ? .allow : .cancel)
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            let requiresLan = Bundle.main.object(forInfoDictionaryKey: "NeptuneLanEndpoint") != nil
            webView.evaluateJavaScript("""
                Boolean(window.Arachnid && document.getElementById("ontology")
                  && (!\(requiresLan) || (window.isSecureContext && window.crypto && window.crypto.subtle)))
                """) { [weak self] ready, _ in
                guard let self else { return }
                self.publish((ready as? Bool) == true ? .ready : .failed(
                    "The scene runtime could not start. Linked scenes require WebCrypto."
                ))
            }
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
            publish(.failed("The bundled scene could not be opened."))
        }

        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
            publish(.failed("The scene could not be loaded."))
        }

        func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
            publish(.failed("The scene stopped. Reload it to continue."))
        }
    }

    // Preserve the compiler's geometry. Scale and center the entire stage on
    // each viewport change rather than changing individual layout or focus data.
    private static let fitViewport = """
        (() => {
          let meta = document.querySelector('meta[name="viewport"]');
          if (!meta) {
            meta = document.createElement('meta');
            meta.name = 'viewport';
            document.head.appendChild(meta);
          }
          meta.content = 'width=device-width, initial-scale=1';
          const stage = document.getElementById('ontology');
          if (!stage) return;
          for (const element of [document.documentElement, document.body]) {
            element.style.width = '100%';
            element.style.height = '100%';
          }
          stage.style.position = 'absolute';
          stage.style.transformOrigin = '0 0';
          function fit() {
            const width = document.documentElement.clientWidth;
            const height = document.documentElement.clientHeight;
            const stageWidth = stage.offsetWidth;
            const stageHeight = stage.offsetHeight;
            if (!width || !height || !stageWidth || !stageHeight) return;
            const scale = Math.min(width / stageWidth, height / stageHeight);
            stage.style.left = ((width - stageWidth * scale) / 2) + 'px';
            stage.style.top = ((height - stageHeight * scale) / 2) + 'px';
            stage.style.transform = 'scale(' + scale + ')';
          }
          window.addEventListener('resize', fit);
          fit();
          window.requestAnimationFrame(fit);
        })();
        """
}
