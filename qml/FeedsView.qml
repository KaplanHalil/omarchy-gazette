import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Feed management: add/remove/toggle subscriptions, assign them to tabs and
// move subscriptions in/out with OPML.
Item {
  id: root

  property var panel: null
  property var feeds: []
  property var tabs: []
  property var pulseSettings: ({})

  signal changed()
  signal refreshRequested()

  property string feedUrl: ""
  property var selectedTabs: ({})
  property string message: ""
  property bool messageIsError: false
  property string pendingDeleteId: ""
  property string opmlPath: Quickshell.env("HOME") + "/pulse-subscriptions.opml"

  readonly property color fg: Commons.Color.foreground
  readonly property color accent: Commons.Color.accent
  readonly property color urgent: Commons.Color.urgent
  readonly property color dim: Qt.rgba(fg.r, fg.g, fg.b, 0.62)
  readonly property color dimmer: Qt.rgba(fg.r, fg.g, fg.b, 0.42)
  readonly property string fontFamily: Style.font.family
  readonly property int errorCount: {
    var n = 0
    for (var i = 0; i < feeds.length; i++) if (String(feeds[i].lastError || "").length > 0) n += 1
    return n
  }
  readonly property bool formFocused: urlField.activeFocus || opmlField.activeFocus

  function focusAdd() {
    urlField.forceActiveFocus()
  }

  function selectedTabIds() {
    var out = []
    for (var key in selectedTabs) if (selectedTabs[key]) out.push(key)
    return out
  }

  function toggleTab(id) {
    var next = {}
    for (var key in selectedTabs) next[key] = selectedTabs[key]
    next[id] = !next[id]
    selectedTabs = next
  }

  function expandPath(path) {
    var value = String(path || "").trim()
    if (value.indexOf("~/") === 0) return Quickshell.env("HOME") + value.substring(1)
    return value
  }

  function showMessage(text, isError) {
    message = text
    messageIsError = isError === true
  }

  function addFeed() {
    var url = feedUrl.trim()
    if (url.length === 0) {
      showMessage("Paste a feed URL or a site homepage.", true)
      return
    }
    var args = ["--add-feed", url]
    var ids = selectedTabIds()
    if (ids.length > 0) args.push("--tabs", ids.join(","))
    showMessage("Adding feed…", false)
    panel.runCommand(args, function(result, code, errorText) {
      if (!result || result.ok === false) {
        showMessage(errorText || "Could not add feed.", true)
        return
      }
      var title = result.feed && result.feed.title ? result.feed.title : url
      showMessage(result.existing ? "Already subscribed — tabs updated (" + title + ")." : "Added “" + title + "”.", false)
      feedUrl = ""
      selectedTabs = ({})
      root.changed()
      root.refreshRequested()
    })
  }

  function removeFeed(id) {
    if (pendingDeleteId !== id) {
      pendingDeleteId = id
      deleteTimer.restart()
      return
    }
    pendingDeleteId = ""
    panel.runCommand(["--remove-feed", id], function(result, code, errorText) {
      if (!result || result.ok === false) {
        showMessage(errorText || "Could not remove feed.", true)
        return
      }
      showMessage("Feed removed.", false)
      root.changed()
    })
  }

  function toggleFeed(feed) {
    panel.runCommand(["--toggle-feed", feed.id], function(result, code, errorText) {
      if (!result || result.ok === false) {
        showMessage(errorText || "Could not update feed.", true)
        return
      }
      root.changed()
    })
  }

  function importOpml() {
    var path = expandPath(opmlPath)
    if (path.length === 0) {
      showMessage("Enter an OPML file path first.", true)
      return
    }
    panel.runCommand(["--import-opml", path], function(result, code, errorText) {
      if (!result || result.ok === false) {
        showMessage(errorText || "Import failed.", true)
        return
      }
      showMessage("Imported " + result.added + " feeds (" + result.skipped + " skipped, " + result.tabsCreated + " tabs).", false)
      root.changed()
      root.refreshRequested()
    })
  }

  function exportOpml() {
    var path = expandPath(opmlPath)
    var args = ["--export-opml"]
    if (path.length > 0) args.push(path)
    panel.runCommand(args, function(result, code, errorText) {
      if (!result || result.ok === false) {
        showMessage(errorText || "Export failed.", true)
        return
      }
      showMessage("Exported to " + result.path, false)
    })
  }

  function seedStarter() {
    panel.runCommand(["--seed-starter"], function(result, code, errorText) {
      if (!result || result.ok === false) {
        showMessage(errorText || "Could not add starter feeds.", true)
        return
      }
      if (result.seeded === false) {
        showMessage("Feeds already exist.", true)
        return
      }
      showMessage("Added " + result.feeds + " starter feeds.", false)
      root.changed()
      root.refreshRequested()
    })
  }

  Timer {
    id: deleteTimer
    interval: 3000
    onTriggered: root.pendingDeleteId = ""
  }

  ColumnLayout {
    anchors.fill: parent
    spacing: Style.space(7)

    // ------------------------------------------------------------ add feed
    RowLayout {
      Layout.fillWidth: true
      Layout.preferredHeight: Style.space(30)
      spacing: Style.space(5)

      TextField {
        id: urlField
        Layout.fillWidth: true
        placeholderText: "https://example.com/feed  or  https://example.com"
        text: root.feedUrl
        foreground: root.fg
        accent: root.accent
        font.pixelSize: Style.font.bodySmall
        verticalPadding: Style.space(4)
        onTextEdited: root.feedUrl = text
        onAccepted: root.addFeed()
        Keys.onEscapePressed: if (root.panel) root.panel.focusList()
      }

      BorderSurface {
        implicitWidth: addLabel.implicitWidth + Style.space(16)
        implicitHeight: Style.space(30)
        radius: Style.cornerRadius
        color: addMouse.containsMouse ? Style.selectedFillFor(root.fg, root.accent) : "transparent"
        borderSpec: Border.controlSpec("normal", root.accent, root.accent)

        Text {
          id: addLabel
          anchors.centerIn: parent
          textFormat: Text.PlainText
          text: "ADD"
          color: root.accent
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
          font.letterSpacing: 0.6
        }

        MouseArea {
          id: addMouse
          anchors.fill: parent
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: root.addFeed()
        }
      }
    }

    RowLayout {
      Layout.fillWidth: true
      Layout.preferredHeight: Style.space(24)
      spacing: Style.space(4)

      Text {
        textFormat: Text.PlainText
        text: "ADD TO"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        font.letterSpacing: 0.4
      }

      Repeater {
        model: root.tabs

        BorderSurface {
          required property var modelData
          implicitWidth: chipLabel.implicitWidth + Style.space(12)
          implicitHeight: Style.space(22)
          radius: Style.cornerRadius
          color: root.selectedTabs[modelData.id] === true
            ? Style.selectedFillFor(root.fg, root.accent)
            : (chipMouse.containsMouse ? Style.hoverFillFor(root.fg, root.accent) : "transparent")
          borderSpec: Border.controlSpec(
            root.selectedTabs[modelData.id] === true ? "selected" : "normal",
            root.fg, root.accent)

          Text {
            id: chipLabel
            anchors.centerIn: parent
            textFormat: Text.PlainText
            text: String(modelData.name || "")
            color: root.selectedTabs[modelData.id] === true ? root.accent : root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
          }

          MouseArea {
            id: chipMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.toggleTab(modelData.id)
          }
        }
      }

      Item { Layout.fillWidth: true }

      Text {
        visible: root.tabs.length === 0
        textFormat: Text.PlainText
        text: "no tabs yet — create them in Tabs"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }
    }

    RowLayout {
      Layout.fillWidth: true
      visible: root.feeds.length === 0
      spacing: Style.space(6)

      BorderSurface {
        implicitWidth: starterLabel.implicitWidth + Style.space(18)
        implicitHeight: Style.space(28)
        radius: Style.cornerRadius
        color: starterMouse.containsMouse ? Style.selectedFillFor(root.fg, root.accent) : "transparent"
        borderSpec: Border.controlSpec("normal", root.accent, root.accent)

        Text {
          id: starterLabel
          anchors.centerIn: parent
          textFormat: Text.PlainText
          text: "ADD STARTER PACK"
          color: root.accent
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
        }

        MouseArea {
          id: starterMouse
          anchors.fill: parent
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: root.seedStarter()
        }
      }

      Text {
        Layout.fillWidth: true
        textFormat: Text.PlainText
        text: "9 feeds across Gaming, Tech, AI and World"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }
    }

    Text {
      visible: root.message.length > 0
      Layout.fillWidth: true
      textFormat: Text.PlainText
      text: root.message
      color: root.messageIsError ? root.urgent : root.accent
      wrapMode: Text.Wrap
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
    }

    PanelSeparator { Layout.fillWidth: true }

    // ------------------------------------------------------------- feed list
    Flickable {
      id: feedList
      Layout.fillWidth: true
      Layout.fillHeight: true
      clip: true
      contentWidth: width
      contentHeight: feedColumn.implicitHeight
      boundsBehavior: Flickable.StopAtBounds
      flickableDirection: Flickable.VerticalFlick
      interactive: contentHeight > height
      ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

      Column {
        id: feedColumn
        width: feedList.width
        spacing: Style.space(5)

        Repeater {
          model: root.feeds

          BorderSurface {
            id: feedRow
            required property var modelData

            width: feedColumn.width
            implicitHeight: Style.space(52)
            radius: Style.cornerRadius
            color: feedMouse.containsMouse ? Style.hoverFillFor(root.fg, root.accent) : "transparent"
            borderSpec: Border.controlSpec("normal", root.fg, root.accent)
            opacity: modelData.enabled ? 1.0 : 0.55

            MouseArea {
              id: feedMouse
              anchors.fill: parent
              hoverEnabled: true
              acceptedButtons: Qt.NoButton
            }

            Column {
              anchors.left: parent.left
              anchors.leftMargin: Style.space(9)
              anchors.right: actions.left
              anchors.rightMargin: Style.space(8)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(2)

              Row {
                spacing: Style.space(6)
                width: parent.width

                Text {
                  textFormat: Text.PlainText
                  text: String(modelData.title || modelData.url || "")
                  color: root.fg
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.bodySmall
                  font.bold: true
                  elide: Text.ElideRight
                  width: Math.min(implicitWidth, feedRow.width - Style.space(180))
                }

                Repeater {
                  model: modelData.tabIds || []

                  Text {
                    required property var modelData
                    textFormat: Text.PlainText
                    text: "[" + root.panel.tabName(modelData) + "]"
                    color: root.dimmer
                    font.family: root.fontFamily
                    font.pixelSize: Style.font.caption
                  }
                }
              }

              Text {
                width: parent.width
                textFormat: Text.PlainText
                text: String(modelData.url || "")
                color: root.dimmer
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                elide: Text.ElideMiddle
              }

              Text {
                width: parent.width
                visible: String(modelData.lastError || "").length > 0
                textFormat: Text.PlainText
                text: "! " + String(modelData.lastError || "")
                color: root.urgent
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                elide: Text.ElideRight
              }
            }

            Row {
              id: actions
              anchors.right: parent.right
              anchors.rightMargin: Style.space(7)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(4)

              ToggleSwitch {
                anchors.verticalCenter: parent.verticalCenter
                checked: modelData.enabled === true
                foreground: root.fg
                accent: root.accent
                onToggled: root.toggleFeed(modelData)
              }

              PanelActionButton {
                anchors.verticalCenter: parent.verticalCenter
                iconText: "\uf00d"
                tooltipText: root.pendingDeleteId === modelData.id ? "Click again to remove" : "Remove feed"
                foreground: root.pendingDeleteId === modelData.id ? root.urgent : root.dimmer
                fontFamily: root.fontFamily
                onClicked: root.removeFeed(modelData.id)
              }
            }
          }
        }

        Item {
          visible: root.feeds.length === 0
          width: parent.width
          height: Style.space(80)

          Text {
            anchors.centerIn: parent
            width: Style.space(360)
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: "No subscriptions yet. Paste any feed URL above, or add the starter pack."
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
          }
        }
      }
    }

    PanelSeparator { Layout.fillWidth: true }

    // ---------------------------------------------------------------- OPML
    RowLayout {
      Layout.fillWidth: true
      Layout.preferredHeight: Style.space(28)
      spacing: Style.space(5)

      TextField {
        id: opmlField
        Layout.fillWidth: true
        placeholderText: "OPML path"
        text: root.opmlPath
        foreground: root.fg
        accent: root.accent
        font.pixelSize: Style.font.caption
        verticalPadding: Style.space(3)
        onTextEdited: root.opmlPath = text
        Keys.onEscapePressed: if (root.panel) root.panel.focusList()
      }

      PanelActionButton {
        iconText: "\uf019"
        tooltipText: "Import OPML"
        foreground: root.fg
        fontFamily: root.fontFamily
        onClicked: root.importOpml()
      }

      PanelActionButton {
        iconText: "\uf093"
        tooltipText: "Export OPML"
        foreground: root.fg
        fontFamily: root.fontFamily
        onClicked: root.exportOpml()
      }

      Text {
        visible: root.errorCount > 0
        textFormat: Text.PlainText
        text: root.errorCount + " feed errors"
        color: root.urgent
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }
    }
  }
}
