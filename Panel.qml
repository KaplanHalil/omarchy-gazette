import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "qml"

// Gazette panel: markets ticker, custom tabs, article list with thumbnails,
// in-panel reader, feed/tab/market/settings management.
//
// Engine access is serialised through one Process queue so a manual refresh,
// the background timer and a settings write can never overlap.
Panel {
  id: root
  moduleName: "io.github.kaplanhalil.gazette"
  ipcTarget: ""
  manageIpc: false

  property var anchorItem: null
  property var hostWidget: null

  property var snapshot: ({
    settings: {},
    tabs: [],
    feeds: [],
    articles: [],
    markets: [],
    marketCatalog: [],
    stats: {},
    paths: {}
  })
  property string engineError: ""
  property string view: "news"
  property string activeTab: "unread"
  property string searchText: ""
  property int cursor: 0
  property string selectedId: ""
  property bool refreshing: false
  property bool loading: false
  property bool _busy: false
  property bool _quiet: false
  property var _queue: []
  property var _callback: null
  property var articleTexts: ({})
  property string articleTextLoading: ""
  property string articleTextError: ""
  property string textOpenFor: ""
  property bool _textRunning: false
  property var _textCallback: null
  property var _textPending: null

  readonly property var gazetteSettings: snapshot.settings || ({})
  readonly property var allArticles: snapshot.articles || []
  readonly property var tabs: snapshot.tabs || []
  readonly property var feeds: snapshot.feeds || []
  readonly property var allMarkets: snapshot.markets || []
  readonly property var marketCatalog: snapshot.marketCatalog || []
  readonly property var enabledMarkets: {
    var out = []
    for (var i = 0; i < allMarkets.length; i++) {
      if (allMarkets[i].enabled) out.push(allMarkets[i])
    }
    return out
  }
  readonly property var stats: snapshot.stats || ({})
  readonly property int unreadCount: Number(stats.unread || 0)
  readonly property int totalCount: Number(stats.total || 0)
  readonly property var ticker: snapshot.barTicker || null
  readonly property string enginePath: {
    var custom = Quickshell.env("GAZETTE_ENGINE")
    if (custom && custom.length > 0) return custom
    return Quickshell.env("HOME") + "/.local/bin/gazette-engine"
  }
  readonly property int intervalSeconds: Math.max(60, (Number(gazetteSettings.refresh_minutes) || 5) * 60)
  readonly property var panelScreen: anchorItem && anchorItem.QsWindow ? anchorItem.QsWindow.window.screen : null
  readonly property bool primaryInstance: panelScreen === null
    || !Quickshell.screens
    || Quickshell.screens.length === 0
    || String(panelScreen.name) === String(Quickshell.screens[0].name)
  readonly property bool typing: newsView.searchFocused
    || feedsView.formFocused
    || tabsView.formFocused
    || marketsView.formFocused
    || settingsView.formFocused

  readonly property color fg: bar ? bar.foreground : Commons.Color.foreground
  readonly property color accent: Commons.Color.accent
  readonly property color urgent: Commons.Color.urgent
  readonly property color dim: Qt.rgba(fg.r, fg.g, fg.b, 0.62)
  readonly property color dimmer: Qt.rgba(fg.r, fg.g, fg.b, 0.42)
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family

  // ------------------------------------------------------------------ engine

  function runCommand(args, callback, quiet) {
    var list = []
    for (var i = 0; i < args.length; i++) list.push(String(args[i]))
    _queue.push({ args: list, callback: callback || null, quiet: quiet === true })
    pump()
  }

  function pump() {
    if (_busy || _queue.length === 0) return
    var job = _queue.shift()
    _callback = job.callback
    _quiet = job.quiet === true
    _busy = true
    cmdProc.command = [enginePath].concat(job.args)
    cmdProc.running = true
  }

  Process {
    id: cmdProc
    stdout: StdioCollector { id: cmdOut; waitForEnd: true }
    stderr: StdioCollector { id: cmdErr; waitForEnd: true }

    onExited: function(code, status) {
      var text = cmdOut.text || ""
      var errText = cmdErr.text || ""
      var parsed = null
      try { parsed = JSON.parse(text) } catch (e) { parsed = null }
      var message = ""
      if (code !== 0 || !parsed || parsed.ok === false) {
        if (parsed && parsed.error) message = String(parsed.error)
        else if (errText.trim().length > 0) message = errText.trim()
        else if (text.trim().length > 0) message = text.trim()
        else message = "engine exited with code " + code
        if (!root._quiet) root.engineError = message
      } else if (!root._quiet) {
        root.engineError = ""
      }
      root._busy = false
      var cb = root._callback
      root._callback = null
      if (cb) cb(parsed, code, message)
      root.pump()
    }
  }

  Process {
    id: notifyProc
  }

  // Full article text runs on its own Process and its own cache file, so it
  // never queues behind a feed refresh.
  Process {
    id: textProc
    stdout: StdioCollector { id: textOut; waitForEnd: true }
    stderr: StdioCollector { id: textErr; waitForEnd: true }

    onExited: function(code, status) {
      var text = textOut.text || ""
      var errText = textErr.text || ""
      var parsed = null
      try { parsed = JSON.parse(text) } catch (e) { parsed = null }
      var message = ""
      if (code !== 0 || !parsed || parsed.ok === false) {
        if (parsed && parsed.error) message = String(parsed.error)
        else if (errText.trim().length > 0) message = errText.trim()
        else message = "could not fetch the full article"
      }
      var cb = root._textCallback
      root._textCallback = null
      root._textRunning = false
      if (cb) cb(parsed, code, message)
      if (root._textPending) {
        var pending = root._textPending
        root._textPending = null
        root.startTextCommand(pending.args, pending.callback)
      }
    }
  }

  function runTextCommand(args, callback) {
    if (_textRunning) {
      _textPending = { args: args, callback: callback }
      return
    }
    startTextCommand(args, callback)
  }

  function startTextCommand(args, callback) {
    _textRunning = true
    _textCallback = callback
    textProc.command = [enginePath].concat(args)
    textProc.running = true
  }

  function reload(callback) {
    loading = true
    runCommand(["--snapshot"], function(result) {
      loading = false
      if (!result || result.ok === false) {
        if (callback) callback(null)
        return
      }
      applySnapshot(result)
      if (callback) callback(result)
    })
  }

  function applySnapshot(result) {
    snapshot = result
    if (activeTab !== "unread" && activeTab !== "all" && activeTab !== "bookmarks") {
      var exists = false
      for (var i = 0; i < tabs.length; i++) if (tabs[i].id === activeTab) exists = true
      if (!exists) activeTab = "unread"
    }
    if (result.pending && result.pending.length > 0) {
      notifyPending(result.pending, result.settings || {})
    }
  }

  function inQuietHours(settings) {
    function toMinutes(value) {
      var parts = String(value || "").split(":")
      if (parts.length !== 2) return -1
      var h = parseInt(parts[0], 10)
      var m = parseInt(parts[1], 10)
      if (!isFinite(h) || !isFinite(m)) return -1
      return h * 60 + m
    }
    var start = toMinutes(settings.quiet_start)
    var end = toMinutes(settings.quiet_end)
    if (start < 0 || end < 0 || start === end) return false
    var now = new Date()
    var current = now.getHours() * 60 + now.getMinutes()
    return start < end
      ? (current >= start && current < end)
      : (current >= start || current < end)
  }

  function notifyPending(pending, settings) {
    var enabled = settings.notifications !== false
    if (enabled && !inQuietHours(settings)) {
      var count = pending.length
      var summary = count + (count === 1 ? " new article" : " new articles")
      var lines = []
      for (var i = 0; i < pending.length && i < 3; i++) {
        lines.push(pending[i].feed + ": " + pending[i].title)
      }
      notifyProc.command = [
        "notify-send",
        "-a", "Gazette",
        "-h", "string:x-canonical-private-synchronous:gazette",
        "-u", "normal",
        "Gazette — " + summary,
        lines.join("\n")
      ]
      notifyProc.running = true
    }
    runCommand(["--ack-notifications"])
  }

  function refreshFeeds() {
    if (refreshing) return
    refreshing = true
    runCommand(["--refresh"], function(result) {
      refreshing = false
      if (!result || result.ok === false) return
      reload(function(updated) {
        if (updated && updated.pending && updated.pending.length > 0) {
          // applySnapshot already queued the notification and acknowledged it.
        }
      })
      refreshMarkets()
    })
  }

  function refreshMarkets() {
    runCommand(["--markets"], function(result) {
      if (result && result.ok) reload()
    })
  }

  function lastRefreshAgeSeconds() {
    var last = Number(stats.lastRefresh || 0)
    if (last <= 0) return 999999
    return Math.max(0, Date.now() / 1000 - last)
  }

  function maybeAutoRefresh() {
    if (lastRefreshAgeSeconds() > intervalSeconds) refreshFeeds()
    else refreshMarkets()
  }

  Timer {
    interval: root.intervalSeconds * 1000
    repeat: true
    running: root.primaryInstance
    onTriggered: root.maybeAutoRefresh()
  }

  Timer {
    interval: Math.max(20, Number(root.gazetteSettings.market_refresh_seconds) || 60) * 1000
    repeat: true
    running: root.opened
    onTriggered: {
      var maxAge = Math.max(15, (Number(root.gazetteSettings.market_refresh_seconds) || 60) - 15)
      if (Date.now() / 1000 - Number(root.snapshot.marketUpdated || 0) > maxAge) root.refreshMarkets()
    }
  }

  Component.onCompleted: {
    reload(function() { if (root.primaryInstance) root.maybeAutoRefresh() })
  }

  // --------------------------------------------------------------- panel API

  function open() {
    if (!opened) controller.show()
    if (view !== "news" && view !== "reader") view = "news"
    if (!snapshot.generatedAt) reload()
    Qt.callLater(function() {
      keyCatcher.forceActiveFocus()
      if (view === "news") newsView.ensureCursor()
    })
    if (lastRefreshAgeSeconds() > intervalSeconds) refreshFeeds()
    if (Date.now() / 1000 - Number(snapshot.marketUpdated || 0) > 45) refreshMarkets()
  }

  function openView(name) {
    view = name
    if (!opened) controller.show()
    Qt.callLater(function() { keyCatcher.forceActiveFocus() })
  }

  onOpenedChanged: {
    if (opened) {
      Qt.callLater(function() { keyCatcher.forceActiveFocus() })
    } else {
      searchText = ""
    }
  }

  // -------------------------------------------------------------- filtering

  function filteredArticles() {
    var out = []
    var source = snapshot.articles || []
    var query = searchText.trim().toLowerCase()
    for (var i = 0; i < source.length; i++) {
      var a = source[i]
      if (activeTab === "unread" && a.read) continue
      if (activeTab === "bookmarks" && !a.bookmarked) continue
      if (activeTab !== "all" && activeTab !== "unread" && activeTab !== "bookmarks") {
        if ((a.tabs || []).indexOf(activeTab) < 0) continue
      }
      if (query.length > 0) {
        var haystack = (String(a.title || "") + " " + String(a.summary || "") + " " + String(a.feedTitle || "")).toLowerCase()
        if (haystack.indexOf(query) < 0) continue
      }
      out.push(a)
    }
    return out
  }

  function articleById(id) {
    var source = snapshot.articles || []
    for (var i = 0; i < source.length; i++) if (source[i].id === id) return source[i]
    return null
  }

  function tabName(id) {
    for (var i = 0; i < tabs.length; i++) if (tabs[i].id === id) return tabs[i].name
    return id
  }

  function focusList() {
    keyCatcher.forceActiveFocus()
    if (view === "news") newsView.ensureCursor()
  }

  function toggleBookmark(article) {
    if (!article) return
    runCommand(["--bookmark", article.id, article.bookmarked ? "0" : "1"], function() { reload() })
  }

  function selectedArticle() {
    return selectedId ? articleById(selectedId) : null
  }

  function unreadForTab(id) {
    if (id === "unread") return unreadCount
    if (id === "all") return totalCount
    if (id === "bookmarks") return Number(stats.bookmarked || 0)
    for (var i = 0; i < tabs.length; i++) if (tabs[i].id === id) return Number(tabs[i].unread || 0)
    return 0
  }

  function switchTab(direction) {
    if (view === "reader") {
      if (direction < 0) view = "news"
      return
    }
    if (view !== "news") return
    var ids = ["unread", "all"]
    for (var i = 0; i < tabs.length; i++) ids.push(tabs[i].id)
    if (Number(stats.bookmarked || 0) > 0) ids.push("bookmarks")
    var current = ids.indexOf(activeTab)
    if (current < 0) current = 0
    activeTab = ids[(current + direction + ids.length) % ids.length]
    cursor = 0
    searchText = ""
    newsView.ensureCursor()
  }

  function moveCursor(dy) {
    if (view === "reader") {
      readerView.scrollBy(dy)
      return
    }
    if (view !== "news") return
    var list = filteredArticles()
    if (list.length === 0) return
    cursor = Math.max(0, Math.min(list.length - 1, cursor + dy))
    newsView.ensureCursor()
  }

  function handleEscape() {
    if (view !== "news") {
      view = "news"
      return
    }
    if (searchText.length > 0) {
      searchText = ""
      return
    }
    close()
  }

  function handleReturn() {
    if (view === "reader") {
      if (!textOpenFor) revealSelectedText()
      return
    }
    if (view !== "news") return
    var list = filteredArticles()
    if (list.length === 0) return
    cursor = Math.max(0, Math.min(list.length - 1, cursor))
    openReader(list[cursor])
  }

  function handleText(text) {
    if (text === "/") {
      view = "news"
      Qt.callLater(function() { newsView.focusSearch() })
    } else if (text === "r" || text === "R") {
      refreshFeeds()
    } else if (text === "s" || text === "S") {
      view = "settings"
    } else if (text === "f" || text === "F") {
      view = "feeds"
    } else if (text === "t" || text === "T") {
      view = "tabs"
    } else if (text === "m" || text === "M") {
      view = "markets"
    } else if (text === "a" || text === "A") {
      markAllRead()
    } else if (text === "o" || text === "O") {
      var article = selectedArticle()
      if (!article && view === "news") {
        var list = filteredArticles()
        if (list.length > 0) article = list[Math.max(0, Math.min(list.length - 1, cursor))]
      }
      if (article) openExternal(article)
    } else if (text === "b" || text === "B") {
      toggleBookmarkSelected()
    } else if (text === "g") {
      if (view === "news") {
        cursor = 0
        newsView.ensureCursor()
      }
    } else if (text === "n" || text === "N") {
      if (view === "tabs") tabsView.startNew()
      else if (view === "feeds") feedsView.focusAdd()
      else if (view === "markets") marketsView.focusAdd()
    }
  }

  function openReader(article) {
    if (!article) return
    selectedId = article.id
    view = "reader"
    articleTextError = ""
    textOpenFor = ""
    if (!article.read) {
      runCommand(["--mark-read", article.id], function() { reload() })
    }
  }

  function fullTextForSelected() {
    if (!selectedId || !articleTexts[selectedId]) return ""
    return String(articleTexts[selectedId].text || "")
  }

  function blocksForSelected() {
    if (!selectedId || !articleTexts[selectedId]) return []
    return articleTexts[selectedId].blocks || []
  }

  function revealSelectedText() {
    var article = selectedArticle()
    if (!article || textOpenFor === article.id) return
    textOpenFor = article.id
    articleTextError = ""
    loadArticleText(article)
  }

  function loadArticleText(article, force) {
    if (!article || !article.id) return
    if (gazetteSettings.full_text === false) return
    if (!force && articleTexts[article.id]) return
    if (!force && articleTextLoading === article.id) return
    if (!article.url) {
      articleTextError = "This article has no web URL."
      return
    }
    articleTextLoading = article.id
    articleTextError = ""
    var command = force ? "--refresh-text" : "--article-text"
    runTextCommand([command, article.id], function(result, code, errorText) {
      if (root.articleTextLoading === article.id) root.articleTextLoading = ""
      if (!result || result.ok === false) {
        if (root.selectedId === article.id) {
          root.articleTextError = errorText || "Full text could not be fetched."
        }
        return
      }
      var next = {}
      for (var key in root.articleTexts) next[key] = root.articleTexts[key]
      next[article.id] = {
        text: String(result.text || ""),
        blocks: result.blocks || []
      }
      root.articleTexts = next
    })
  }

  function openExternal(article) {
    if (!article || !article.url) return
    Qt.openUrlExternally(article.url)
    if (!article.read) runCommand(["--mark-read", article.id], function() { reload() })
  }

  function toggleBookmarkSelected() {
    var article = selectedArticle()
    if (!article && view === "news") {
      var list = filteredArticles()
      if (list.length > 0) article = list[Math.max(0, Math.min(list.length - 1, cursor))]
    }
    if (!article) return
    runCommand(["--bookmark", article.id, article.bookmarked ? "0" : "1"], function() { reload() })
  }

  function markAllRead() {
    if (activeTab === "bookmarks") return
    var args = ["--mark-all-read"]
    if (activeTab !== "unread" && activeTab !== "all") args.push("--tab", activeTab)
    runCommand(args, function() { reload() })
  }

  function saveSettings(patch) {
    runCommand(["--save-settings", JSON.stringify(patch)], function() { reload() })
  }

  // ------------------------------------------------------------------- chrome

  readonly property string viewTitle: {
    if (view === "reader") return "Reader"
    if (view === "feeds") return "Feeds"
    if (view === "tabs") return "Tabs"
    if (view === "markets") return "Markets"
    if (view === "settings") return "Settings"
    return "Gazette"
  }

  readonly property string metaLine: {
    if (view === "feeds") return feeds.length + " FEEDS · " + stats.errors + " ERRORS"
    if (view === "tabs") return tabs.length + " TABS · " + unreadCount + " UNREAD"
    if (view === "markets") return enabledMarkets.length + " TICKERS · " + (snapshot.marketUpdated ? "LIVE" : "NO DATA")
    if (view === "settings") return "GAZETTE " + String(snapshot.version || "1.0.0")
    if (view === "reader") {
      var a = selectedArticle()
      return a ? String(a.feedTitle || "FEED").toUpperCase() : "ARTICLE"
    }
    return unreadCount + " UNREAD · " + feeds.length + " FEEDS · " + tabs.length + " TABS"
  }

  KeyboardPanel {
    id: keyboard
    anchorItem: root.anchorItem
    owner: root.hostWidget || root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: keyboard.fittedContentWidth(Style.space(660), Style.space(720))
    contentHeight: keyboard.fittedContentHeight(Style.space(860), Style.space(920))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      blocked: root.typing

      onMoveRequested: function(dx, dy) {
        if (root.view === "news" && dx !== 0) root.switchTab(dx)
        else root.moveCursor(dy)
      }
      onReturnRequested: root.handleReturn()
      onCloseRequested: root.handleEscape()
      onTabRequested: function(direction) { root.switchTab(direction) }
      onTextKey: function(text) { root.handleText(text) }
      onDeleteRequested: root.toggleBookmarkSelected()

      ColumnLayout {
        anchors.fill: parent
        spacing: 0

        MarketsStrip {
          Layout.fillWidth: true
          Layout.preferredHeight: Style.space(30)
          visible: root.enabledMarkets.length > 0
          markets: root.enabledMarkets
          updated: Number(root.snapshot.marketUpdated || 0)
          onActivated: root.view = (root.view === "markets" ? "news" : "markets")
        }

        PanelSeparator {
          Layout.fillWidth: true
          visible: root.enabledMarkets.length > 0
        }

        RowLayout {
          Layout.fillWidth: true
          Layout.topMargin: Style.space(9)
          Layout.bottomMargin: Style.space(8)
          spacing: Style.space(9)

          Text {
            textFormat: Text.PlainText
            text: "\uf09e"
            color: root.fg
            font.family: root.fontFamily
            font.pixelSize: Style.font.display
            opacity: 0.9
          }

          ColumnLayout {
            Layout.fillWidth: true
            spacing: Style.space(1)

            Text {
              Layout.fillWidth: true
              textFormat: Text.PlainText
              text: root.viewTitle
              color: root.fg
              font.family: root.fontFamily
              font.pixelSize: Style.font.title
              font.bold: true
              elide: Text.ElideRight
            }

            Text {
              Layout.fillWidth: true
              textFormat: Text.PlainText
              text: root.metaLine
              color: root.dimmer
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              font.letterSpacing: 0.6
              elide: Text.ElideRight
            }
          }

          Row {
            spacing: Style.space(1)

            PanelActionButton {
              visible: root.view !== "news" && root.view !== "reader"
              iconText: "\uf060"
              tooltipText: "Back"
              foreground: root.fg
              fontFamily: root.fontFamily
              onClicked: root.view = "news"
            }

            PanelActionButton {
              visible: root.view === "news"
              iconText: "\uf0ac"
              tooltipText: "Feeds"
              foreground: root.fg
              fontFamily: root.fontFamily
              onClicked: root.view = "feeds"
            }

            PanelActionButton {
              visible: root.view === "news"
              iconText: "\uf0b0"
              tooltipText: "Tabs"
              foreground: root.fg
              fontFamily: root.fontFamily
              onClicked: root.view = "tabs"
            }

            PanelActionButton {
              visible: root.view === "news"
              iconText: "\uf021"
              tooltipText: "Refresh feeds"
              foreground: root.fg
              fontFamily: root.fontFamily
              onClicked: root.refreshFeeds()
            }

            PanelActionButton {
              visible: root.view === "news"
              iconText: "\uf00c"
              tooltipText: "Mark all read"
              foreground: root.fg
              fontFamily: root.fontFamily
              onClicked: root.markAllRead()
            }

            PanelActionButton {
              iconText: "\uf013"
              tooltipText: "Settings"
              foreground: root.fg
              fontFamily: root.fontFamily
              onClicked: root.view = (root.view === "settings" ? "news" : "settings")
            }
          }
        }

        PanelSeparator { Layout.fillWidth: true }

        Text {
          visible: root.engineError.length > 0
          Layout.fillWidth: true
          Layout.topMargin: Style.space(6)
          textFormat: Text.PlainText
          text: "Engine: " + root.engineError
          color: root.urgent
          wrapMode: Text.Wrap
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
        }

        Item {
          id: viewHost
          Layout.fillWidth: true
          Layout.fillHeight: true
          Layout.topMargin: Style.space(8)

          NewsView {
            id: newsView
            anchors.fill: parent
            visible: root.view === "news"
            panel: root
            articles: root.filteredArticles()
            tabs: root.tabs
            gazetteSettings: root.gazetteSettings
            activeTab: root.activeTab
            unreadCount: root.unreadCount
            totalCount: root.totalCount
            cursor: root.cursor
            search: root.searchText
            refreshing: root.refreshing
            onTabPicked: function(id) { root.activeTab = id; root.cursor = 0; root.searchText = "" }
            onAddTabRequested: { root.view = "tabs"; tabsView.startNew() }
            onSearchEdited: function(text) { root.searchText = text; root.cursor = 0 }
            onCursorMoved: function(index) { root.cursor = index }
            onActivateAt: function(index) {
              var list = root.filteredArticles()
              if (index >= 0 && index < list.length) {
                root.cursor = index
                root.openReader(list[index])
              }
            }
            onRefreshRequested: root.refreshFeeds()
            onSettingsRequested: root.view = "settings"
            onImagesToggled: function(enabled) { root.saveSettings({ show_images: enabled }) }
            onClearSearchRequested: root.searchText = ""
          }

          ReaderView {
            id: readerView
            anchors.fill: parent
            visible: root.view === "reader"
            article: root.selectedArticle()
            panel: root
            showImages: root.gazetteSettings.show_images !== false
            fullText: root.fullTextForSelected()
            fullTextBlocks: root.blocksForSelected()
            fullTextRevealed: root.textOpenFor === root.selectedId
            fullTextLoading: root.articleTextLoading === root.selectedId
            fullTextError: root.articleTextError
            fullTextEnabled: root.gazetteSettings.full_text !== false
            onBackRequested: root.view = "news"
            onLoadFullTextRequested: root.revealSelectedText()
            onReloadTextRequested: root.loadArticleText(root.selectedArticle(), true)
            onOpenExternalRequested: function(article) { root.openExternal(article) }
            onToggleBookmarkRequested: function(article) { root.toggleBookmarkSelected() }
            onToggleReadRequested: function(article) {
              root.runCommand([article.read ? "--mark-unread" : "--mark-read", article.id], function() { root.reload() })
            }
          }

          FeedsView {
            id: feedsView
            anchors.fill: parent
            visible: root.view === "feeds"
            panel: root
            feeds: root.feeds
            tabs: root.tabs
            gazetteSettings: root.gazetteSettings
            onChanged: root.reload()
            onRefreshRequested: root.refreshFeeds()
          }

          TabsView {
            id: tabsView
            anchors.fill: parent
            visible: root.view === "tabs"
            panel: root
            tabs: root.tabs
            feeds: root.feeds
            gazetteSettings: root.gazetteSettings
            onChanged: root.reload()
          }

          MarketsView {
            id: marketsView
            anchors.fill: parent
            visible: root.view === "markets"
            panel: root
            markets: root.allMarkets
            catalog: root.marketCatalog
            gazetteSettings: root.gazetteSettings
            marketUpdated: Number(root.snapshot.marketUpdated || 0)
            onChanged: root.reload()
            onRefreshRequested: root.refreshMarkets()
          }

          SettingsView {
            id: settingsView
            anchors.fill: parent
            visible: root.view === "settings"
            panel: root
            gazetteSettings: root.gazetteSettings
            stats: root.stats
            paths: root.snapshot.paths || ({})
            version: String(root.snapshot.version || "1.0.0")
            onChanged: root.reload()
            onRefreshRequested: root.refreshFeeds()
          }
        }
      }
    }
  }
}
