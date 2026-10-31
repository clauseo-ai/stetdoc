// Screenshots one URL with the system WebKit (WKWebView). Nothing is downloaded; it uses the WebKit the Mac ships.
//
// Usage (macOS):
//   swift site/scripts/webkit-shot.swift --url URL --width W [--height H] [--dark] [--full] --out PATH.png [--wait-ms N]
//
//   --width W    layout width in points; the page's window.innerWidth is W
//   --height H   viewport height (default 900); in --full mode it is only the starting size
//   --dark       forces the dark appearance, so prefers-color-scheme: dark matches (default: light)
//   --full       whole page: measures scrollHeight, grows the webview to it (capped at 16000 px), snapshots it all
//   --out PATH   the PNG, written at 1x so it is W pixels wide
//   --wait-ms N  wait after the load and the fonts, for the page's own animations (default 400)
//   --eval JS    a script body run after the load (and the finish); its return value is printed as eval=...
//   --no-finish  leave the page's time-based animations where they are. By default they are finished before the
//                snapshot: the window sits off screen, WebKit treats the page as hidden and never ticks them, so
//                without this every load animation would be caught at its first frame. Scroll-driven animations
//                are left alone; the line printed says whether this WebKit supports animation-timeline: view().
//
// Prints one line: PATH WxH innerWidth=N prefers-dark=BOOL. Exits 1 on a load failure, a WebKit failure or a 30 s
// load timeout, or an HTTP status of 400 or more, and 2 on bad arguments. Scroll-driven animations are judged at the top of the page: the page is
// scrolled to 0 before each snapshot.
import Cocoa
import WebKit

let fullHeightCap: CGFloat = 16000

func fail(_ message: String, code: Int32 = 1) -> Never {
  FileHandle.standardError.write(Data("webkit-shot: \(message)\n".utf8))
  exit(code)
}

struct Options {
  var url: URL
  var width: CGFloat
  var height: CGFloat
  var dark: Bool
  var full: Bool
  var out: String
  var waitMs: Int
  var finish: Bool
  var eval: String?
}

func parse() -> Options {
  let args = Array(CommandLine.arguments.dropFirst())
  var url: URL?
  var width: CGFloat?
  var height: CGFloat = 900
  var dark = false
  var full = false
  var out: String?
  var waitMs = 400
  var finish = true
  var eval: String?
  var i = 0
  func value(_ flag: String) -> String {
    i += 1
    guard i < args.count else { fail("\(flag) needs a value", code: 2) }
    return args[i]
  }
  func number(_ flag: String) -> Double {
    guard let n = Double(value(flag)), n > 0 else { fail("\(flag) needs a positive number", code: 2) }
    return n
  }
  while i < args.count {
    switch args[i] {
    case "--url":
      let text = value("--url")
      guard let u = URL(string: text), u.scheme == "http" || u.scheme == "https" || u.scheme == "file" else {
        fail("--url needs an http, https or file URL, got \(text)", code: 2)
      }
      url = u
    case "--width": width = CGFloat(number("--width"))
    case "--height": height = CGFloat(number("--height"))
    case "--dark": dark = true
    case "--full": full = true
    case "--out": out = value("--out")
    case "--no-finish": finish = false
    case "--eval": eval = value("--eval")
    case "--wait-ms": waitMs = Int(value("--wait-ms")) ?? -1
    case "-h", "--help":
      print("usage: webkit-shot.swift --url URL --width W [--height H] [--dark] [--full] --out PATH.png [--wait-ms N]")
      exit(0)
    default: fail("unknown argument \(args[i])", code: 2)
    }
    i += 1
  }
  guard let url else { fail("--url is required", code: 2) }
  guard let width else { fail("--width is required", code: 2) }
  guard let out else { fail("--out is required", code: 2) }
  guard waitMs >= 0 else { fail("--wait-ms needs a whole number of milliseconds", code: 2) }
  return Options(url: url, width: width, height: height, dark: dark, full: full, out: out, waitMs: waitMs, finish: finish, eval: eval)
}

final class Shooter: NSObject, WKNavigationDelegate {
  let opt: Options
  let window: NSWindow
  let web: WKWebView
  var loaded = false
  var innerWidth = 0
  var prefersDark = false
  var capped = false
  var viewTimeline = false
  var finished = 0
  var evaluated = false

  init(_ opt: Options) {
    self.opt = opt
    let frame = NSRect(x: 0, y: 0, width: opt.width, height: opt.height)
    web = WKWebView(frame: frame, configuration: WKWebViewConfiguration())
    // A borderless window parked far off screen: WebKit lays out and paints it, but nobody sees it.
    window = NSWindow(contentRect: frame, styleMask: [.borderless], backing: .buffered, defer: false)
    super.init()
    let appearance = NSAppearance(named: opt.dark ? .darkAqua : .aqua)
    window.appearance = appearance
    web.appearance = appearance
    window.isReleasedWhenClosed = false
    window.contentView = web
    window.setFrameOrigin(NSPoint(x: -30000, y: -30000))
    web.navigationDelegate = self
  }

  func start() {
    window.orderBack(nil)
    web.load(URLRequest(url: opt.url, cachePolicy: .reloadIgnoringLocalCacheData))
    DispatchQueue.main.asyncAfter(deadline: .now() + 30) {
      if !self.loaded { fail("no load within 30 s: \(self.opt.url.absoluteString)") }
    }
  }

  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
    guard !loaded else { return }
    loaded = true
    settle { self.measure() }
  }

  /// An HTTP error page (404, 500) is not a screenshot of the page the caller asked for, so it fails the run.
  func webView(_ webView: WKWebView, decidePolicyFor navigationResponse: WKNavigationResponse,
               decisionHandler: @escaping (WKNavigationResponsePolicy) -> Void) {
    if navigationResponse.isForMainFrame, let http = navigationResponse.response as? HTTPURLResponse, http.statusCode >= 400 {
      fail("HTTP \(http.statusCode) for \(opt.url.absoluteString)")
    }
    decisionHandler(.allow)
  }

  func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
    fail("could not load \(opt.url.absoluteString): \(error.localizedDescription)")
  }

  func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
    fail("failed while loading \(opt.url.absoluteString): \(error.localizedDescription)")
  }

  func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
    fail("the web content process was lost while loading \(opt.url.absoluteString)")
  }

  /// Waits for the web fonts (at most 8 s), then the caller's delay, then runs `next`.
  func settle(delayMs: Int? = nil, then next: @escaping () -> Void) {
    let js = "await Promise.race([document.fonts.ready, new Promise(function (r) { setTimeout(r, 8000); })]); return true;"
    web.callAsyncJavaScript(js, arguments: [:], in: nil, in: .page) { result in
      if case .failure(let error) = result { fail("could not wait for the fonts: \(error.localizedDescription)") }
      DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(delayMs ?? self.opt.waitMs)) { next() }
    }
  }

  func run(_ js: String, then next: @escaping (Any?) -> Void) {
    web.callAsyncJavaScript(js, arguments: [:], in: nil, in: .page) { result in
      switch result {
      case .success(let value): next(value)
      case .failure(let error): fail("page script failed: \(error.localizedDescription)")
      }
    }
  }

  /// Reads the layout facts, scrolls to the top, and goes on to the snapshot.
  func measure() {
    let js = """
    window.scrollTo(0, 0);
    var de = document.documentElement;
    var finished = 0;
    if (\(opt.finish ? "true" : "false")) {
      document.getAnimations().forEach(function (a) {
        if (a.timeline && a.timeline !== document.timeline) return;
        try { a.finish(); finished++; } catch (e) {}
      });
    }
    return JSON.stringify({
      viewTimeline: CSS.supports('animation-timeline: view()'),
      finished: finished,
      innerWidth: window.innerWidth,
      scrollHeight: de.scrollHeight,
      dark: window.matchMedia('(prefers-color-scheme: dark)').matches
    });
    """
    run(js) { value in
      guard let text = value as? String, let data = text.data(using: .utf8),
            let facts = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let inner = facts["innerWidth"] as? NSNumber, let scroll = facts["scrollHeight"] as? NSNumber,
            let dark = facts["dark"] as? Bool else {
        fail("could not read the page's size and colour scheme")
      }
      self.innerWidth = inner.intValue
      self.prefersDark = dark
      self.viewTimeline = (facts["viewTimeline"] as? Bool) ?? false
      self.finished = (facts["finished"] as? NSNumber)?.intValue ?? 0
      if self.opt.full {
        var height = max(CGFloat(scroll.doubleValue.rounded(.up)), self.opt.height)
        if height > fullHeightCap { height = fullHeightCap; self.capped = true }
        self.resize(to: height) { self.refit(tries: 3) }
      } else {
        self.snapshot()
      }
    }
  }

  /// Once the view is as tall as the page there is no scrollbar, so the page lays out wider and its height changes
/// (WebKit on a Mac with a mouse draws classic scrollbars that take 15 px; iPhone and iPad never do);
  /// measure again and refit until the height holds (at most `tries` more times).
  func refit(tries: Int) {
    // the content's own height, measured with the body's min-height lifted: a body with min-height: 100vh would
    // otherwise hold the first, taller guess
    let js = "var s = document.body.style, m = s.minHeight; s.minHeight = '0px'; var b = 0; for (var e of document.body.children) { var r = e.getBoundingClientRect(); if (r.height) b = Math.max(b, r.bottom + window.scrollY); } b += parseFloat(getComputedStyle(document.body).marginBottom) || 0; s.minHeight = m; return Math.ceil(b);"
    run(js) { value in
      var height = CGFloat((value as? NSNumber)?.doubleValue ?? 0)
      if height > fullHeightCap { height = fullHeightCap; self.capped = true }
      if tries > 0, height > 0, abs(height - self.web.bounds.height) > 1 {
        self.resize(to: max(height, self.opt.height)) { self.refit(tries: tries - 1) }
      } else {
        self.snapshot()
      }
    }
  }

  func resize(to height: CGFloat, then next: @escaping () -> Void) {
    window.setContentSize(NSSize(width: opt.width, height: height))
    web.frame = NSRect(x: 0, y: 0, width: opt.width, height: height)
    web.layoutSubtreeIfNeeded()
    settle(delayMs: 300) {
      self.run("window.scrollTo(0, 0); return true;") { _ in next() }
    }
  }

  func snapshot() {
    if let js = opt.eval, !evaluated {
      evaluated = true
      run(js) { value in
        print("eval=\(value.map { "\($0)" } ?? "nil")")
        self.snapshot()
      }
      return
    }
    let config = WKSnapshotConfiguration()
    config.rect = web.bounds
    config.snapshotWidth = NSNumber(value: Double(opt.width))
    web.takeSnapshot(with: config) { image, error in
      guard let image else { fail("snapshot failed: \(error?.localizedDescription ?? "no image was returned")") }
      self.save(image)
    }
  }

  /// Writes the snapshot as a PNG at exactly one pixel per point: W pixels wide.
  func save(_ image: NSImage) {
    let pixelsWide = Int(opt.width.rounded())
    let pixelsHigh = Int(web.bounds.height.rounded())
    guard let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixelsWide, pixelsHigh: pixelsHigh,
                                     bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                                     colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
          let context = NSGraphicsContext(bitmapImageRep: rep) else {
      fail("could not make a \(pixelsWide)x\(pixelsHigh) bitmap")
    }
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = context
    context.imageInterpolation = .high
    image.draw(in: NSRect(x: 0, y: 0, width: pixelsWide, height: pixelsHigh), from: .zero, operation: .copy, fraction: 1)
    NSGraphicsContext.restoreGraphicsState()
    guard let png = rep.representation(using: .png, properties: [:]) else { fail("could not encode the PNG") }
    let outURL = URL(fileURLWithPath: opt.out)
    do {
      try FileManager.default.createDirectory(at: outURL.deletingLastPathComponent(), withIntermediateDirectories: true)
      try png.write(to: outURL)
    } catch {
      fail("could not write \(opt.out): \(error.localizedDescription)")
    }
    var line = "\(opt.out) \(pixelsWide)x\(pixelsHigh) innerWidth=\(innerWidth) prefers-dark=\(prefersDark) view-timeline=\(viewTimeline) finished=\(finished)"
    if opt.full { line += capped ? " (full page, capped at \(Int(fullHeightCap)) px)" : " (full page)" }
    print(line)
    exit(0)
  }
}

let opt = parse()
// iPhones and iPads overlay their scrollbars, so a page lays out at the full width. A Mac with a mouse draws classic
// scrollbars that take about 15 px from the layout and from the width media queries see, so shots taken there would
// land in the wrong breakpoint near a band edge. Overlay scrollbars for this process only; nothing is saved.
UserDefaults.standard.setVolatileDomain(["AppleShowScrollBars": "WhenScrolling"], forName: UserDefaults.argumentDomain)
let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
let shooter = Shooter(opt)
DispatchQueue.main.async { shooter.start() }
app.run()
