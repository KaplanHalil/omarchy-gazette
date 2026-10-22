import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Pulse bar widget: RSS icon with an unread dot and an optional market ticker.
// All engine work lives in Panel.qml, which stays loaded even while hidden, so
// the ticker and unread count keep updating when the panel is closed.
BarWidget {
  id: root
  moduleName: "io.github.kaplanhalil.pulse"

  readonly property var panel: panelLoader.item
  readonly property int unread: panel ? panel.unreadCount : 0
  readonly property var ticker: panel ? panel.ticker : null
  readonly property bool tickerVisible: !root.vertical
    && panel !== null
    && panel.pulseSettings.bar_ticker !== false
    && ticker !== null
    && ticker !== undefined
  readonly property color fg: bar ? bar.foreground : Commons.Color.foreground
  readonly property color accent: Commons.Color.accent
  readonly property color urgent: Commons.Color.urgent

  function open() {
    if (panel) panel.open()
    else Qt.callLater(root.open)
  }
  function close() { if (panel) panel.close() }
  function toggle() { opened ? close() : open() }
  readonly property bool opened: panel ? panel.opened === true : false
  function closeForPopoutSwitch() { if (panel) panel.closeForPopoutSwitch() }

  function injectPanel() {
    var target = panelLoader.item
    if (!target) return
    if ("bar" in target) target.bar = root.bar
    if ("settings" in target) target.settings = root.settings
    if ("anchorItem" in target) target.anchorItem = iconSlot
    if ("hostWidget" in target) target.hostWidget = root
  }

  implicitWidth: row.implicitWidth + Style.space(6)
  implicitHeight: bar ? bar.barSize : Style.bar.sizeHorizontal

  Row {
    id: row
    anchors.centerIn: parent
    spacing: Style.space(5)

    Item {
      id: iconSlot
      width: Style.bar.iconSlot
      height: root.implicitHeight

      BarIconButton {
        anchors.centerIn: parent
        bar: root.bar
        text: "\uf09e"
        fontFamily: Style.font.family
        foreground: root.fg
        tooltipText: root.unread > 0
          ? (root.unread + (root.unread === 1 ? " unread article" : " unread articles"))
          : "Pulse — all caught up"
        onPressed: function(button) {
          if (button === Qt.RightButton) {
            if (panel) panel.refreshFeeds()
          } else {
            root.toggle()
          }
        }
      }

      Rectangle {
        visible: root.unread > 0
        width: Style.space(5)
        height: width
        radius: width / 2
        color: root.accent
        anchors.top: parent.top
        anchors.topMargin: Style.space(3)
        anchors.right: parent.right
        anchors.rightMargin: Style.space(4)
      }
    }

    Text {
      id: tickerText
      visible: root.tickerVisible
      anchors.verticalCenter: parent.verticalCenter
      textFormat: Text.PlainText
      text: {
        if (!root.ticker) return ""
        var label = String(root.ticker.label || "")
        var price = String(root.ticker.priceText || "")
        return label.length > 0 ? label + " " + price : price
      }
      color: root.ticker && root.ticker.stale ? Qt.rgba(root.fg.r, root.fg.g, root.fg.b, 0.55) : root.fg
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
      font.bold: false

      MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.toggle()
      }
    }

    // Tiny up/down marker keeps the bar compact while still carrying the
    // direction of the day's move.
    Text {
      visible: root.tickerVisible && root.ticker
      anchors.verticalCenter: parent.verticalCenter
      textFormat: Text.PlainText
      text: {
        if (!root.ticker) return ""
        var pct = Number(root.ticker.changePct || 0)
        if (pct > 0.005) return "▲"
        if (pct < -0.005) return "▼"
        return "·"
      }
      color: {
        if (!root.ticker) return root.fg
        if (Number(root.ticker.changePct) > 0.005) return root.accent
        if (Number(root.ticker.changePct) < -0.005) return root.urgent
        return Qt.rgba(root.fg.r, root.fg.g, root.fg.b, 0.5)
      }
      font.family: Style.font.family
      font.pixelSize: Style.font.caption
    }
  }

  Loader {
    id: panelLoader
    active: true
    visible: false
    source: Qt.resolvedUrl("Panel.qml")
    onLoaded: {
      root.injectPanel()
      Qt.callLater(root.injectPanel)
    }
  }

  onBarChanged: injectPanel()
  onSettingsChanged: injectPanel()

  IpcHandler {
    target: root.moduleName

    function open(): void { root.open() }
    function close(): void { root.close() }
    function show(): void { root.open() }
    function hide(): void { root.close() }
    function toggle(): void { root.toggle() }
    function refresh(): void { if (panel) panel.refreshFeeds() }
    function settings(): void { if (panel) panel.openView("settings") }
    function feeds(): void { if (panel) panel.openView("feeds") }
    function tabs(): void { if (panel) panel.openView("tabs") }
    function markets(): void { if (panel) panel.openView("markets") }
    function news(): void { if (panel) panel.openView("news") }
    function detail(id: string): void {
      if (!panel) return
      panel.openView("news")
      var article = panel.articleById(id)
      if (article) panel.openReader(article)
    }
  }
}
