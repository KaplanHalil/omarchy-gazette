import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Tab manager. Tabs are the user's own categories: a name, an ordered feed
// list, optional keyword include/exclude rules and a notification flag.
Item {
  id: root

  property var panel: null
  property var tabs: []
  property var feeds: []
  property var pulseSettings: ({})

  signal changed()

  property bool editing: false
  property string editingId: ""
  property string draftName: ""
  property string draftGlyph: ""
  property string draftInclude: ""
  property string draftExclude: ""
  property bool draftNotify: true
  property var draftFeeds: ({})
  property string message: ""
  property bool messageIsError: false
  property string pendingDeleteId: ""

  readonly property color fg: Commons.Color.foreground
  readonly property color accent: Commons.Color.accent
  readonly property color urgent: Commons.Color.urgent
  readonly property color dim: Qt.rgba(fg.r, fg.g, fg.b, 0.62)
  readonly property color dimmer: Qt.rgba(fg.r, fg.g, fg.b, 0.42)
  readonly property string fontFamily: Style.font.family
  readonly property bool formFocused: editing && (
    nameField.activeFocus || glyphField.activeFocus
    || includeField.activeFocus || excludeField.activeFocus)

  function startNew() {
    editing = true
    editingId = ""
    draftName = ""
    draftGlyph = ""
    draftInclude = ""
    draftExclude = ""
    draftNotify = true
    draftFeeds = ({})
    message = ""
    Qt.callLater(function() { nameField.forceActiveFocus() })
  }

  function editTab(tab) {
    editing = true
    editingId = String(tab.id || "")
    draftName = String(tab.name || "")
    draftGlyph = String(tab.glyph || "")
    var include = []
    var exclude = []
    for (var i = 0; i < (tab.include || []).length; i++) include.push(tab.include[i])
    for (var j = 0; j < (tab.exclude || []).length; j++) exclude.push(tab.exclude[j])
    draftInclude = include.join(", ")
    draftExclude = exclude.join(", ")
    draftNotify = tab.notify !== false
    var selected = {}
    for (var f = 0; f < (tab.feeds || []).length; f++) selected[tab.feeds[f]] = true
    draftFeeds = selected
    message = ""
    Qt.callLater(function() { nameField.forceActiveFocus() })
  }

  function cancelEdit() {
    editing = false
    editingId = ""
    message = ""
  }

  function toggleDraftFeed(id) {
    var next = {}
    for (var key in draftFeeds) next[key] = draftFeeds[key]
    next[id] = !next[id]
    draftFeeds = next
  }

  function splitKeywords(text) {
    var out = []
    var parts = String(text || "").split(",")
    for (var i = 0; i < parts.length; i++) {
      var value = parts[i].trim()
      if (value.length > 0) out.push(value)
    }
    return out
  }

  function saveTab() {
    if (draftName.trim().length === 0) {
      message = "Give the tab a name."
      messageIsError = true
      return
    }
    var feedIds = []
    for (var key in draftFeeds) if (draftFeeds[key]) feedIds.push(key)
    var payload = {
      id: editingId,
      name: draftName.trim(),
      glyph: draftGlyph,
      feeds: feedIds,
      include: splitKeywords(draftInclude),
      exclude: splitKeywords(draftExclude),
      notify: draftNotify,
      images: null
    }
    panel.runCommand(["--save-tab", JSON.stringify(payload)], function(result, code, errorText) {
      if (!result || result.ok === false) {
        message = errorText || "Could not save tab."
        messageIsError = true
        return
      }
      editing = false
      editingId = ""
      message = ""
      root.changed()
    })
  }

  function removeTab(id) {
    if (pendingDeleteId !== id) {
      pendingDeleteId = id
      deleteTimer.restart()
      return
    }
    pendingDeleteId = ""
    panel.runCommand(["--delete-tab", id], function(result, code, errorText) {
      if (!result || result.ok === false) {
        message = errorText || "Could not delete tab."
        messageIsError = true
        return
      }
      root.changed()
    })
  }

  function moveTab(id, direction) {
    panel.runCommand(["--move-tab", id, direction], function(result, code, errorText) {
      if (!result || result.ok === false) {
        message = errorText || "Could not reorder tabs."
        messageIsError = true
        return
      }
      root.changed()
    })
  }

  Timer {
    id: deleteTimer
    interval: 3000
    onTriggered: root.pendingDeleteId = ""
  }

  ColumnLayout {
    anchors.fill: parent
    spacing: Style.space(6)
    visible: !root.editing

    RowLayout {
      Layout.fillWidth: true
      Layout.preferredHeight: Style.space(28)
      spacing: Style.space(6)

      Text {
        Layout.fillWidth: true
        textFormat: Text.PlainText
        text: root.tabs.length + (root.tabs.length === 1 ? " CUSTOM TAB" : " CUSTOM TABS")
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        font.letterSpacing: 0.5
      }

      BorderSurface {
        implicitWidth: newLabel.implicitWidth + Style.space(16)
        implicitHeight: Style.space(26)
        radius: Style.cornerRadius
        color: newMouse.containsMouse ? Style.selectedFillFor(root.fg, root.accent) : "transparent"
        borderSpec: Border.controlSpec("normal", root.accent, root.accent)

        Text {
          id: newLabel
          anchors.centerIn: parent
          textFormat: Text.PlainText
          text: "+ NEW TAB"
          color: root.accent
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
        }

        MouseArea {
          id: newMouse
          anchors.fill: parent
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: root.startNew()
        }
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

    Flickable {
      id: tabList
      Layout.fillWidth: true
      Layout.fillHeight: true
      clip: true
      contentWidth: width
      contentHeight: tabColumn.implicitHeight
      boundsBehavior: Flickable.StopAtBounds
      flickableDirection: Flickable.VerticalFlick
      interactive: contentHeight > height
      ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

      Column {
        id: tabColumn
        width: tabList.width
        spacing: Style.space(5)

        Repeater {
          model: root.tabs

          BorderSurface {
            id: tabRow
            required property var modelData
            required property int index

            width: tabColumn.width
            implicitHeight: Style.space(48)
            radius: Style.cornerRadius
            color: tabMouse.containsMouse ? Style.hoverFillFor(root.fg, root.accent) : "transparent"
            borderSpec: Border.controlSpec("normal", root.fg, root.accent)

            MouseArea {
              id: tabMouse
              anchors.fill: parent
              hoverEnabled: true
              acceptedButtons: Qt.NoButton
            }

            Column {
              anchors.left: parent.left
              anchors.leftMargin: Style.space(9)
              anchors.right: actions.right
              anchors.rightMargin: Style.space(10)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(2)

              Row {
                spacing: Style.space(7)

                Text {
                  textFormat: Text.PlainText
                  text: String(modelData.name || "")
                  color: root.fg
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.bodySmall
                  font.bold: true
                }

                Text {
                  textFormat: Text.PlainText
                  text: modelData.unread + " unread · " + modelData.total + " total"
                  color: root.dimmer
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.caption
                }
              }

              Text {
                width: parent.width
                textFormat: Text.PlainText
                text: {
                  var parts = []
                  if ((modelData.feeds || []).length > 0) parts.push(modelData.feeds.length + " feeds")
                  if ((modelData.include || []).length > 0) parts.push("+" + modelData.include.join(", +"))
                  if ((modelData.exclude || []).length > 0) parts.push("-" + modelData.exclude.join(", -"))
                  if (!modelData.notify) parts.push("quiet")
                  return parts.length > 0 ? parts.join(" · ") : "no feeds selected"
                }
                color: root.dimmer
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                elide: Text.ElideRight
              }
            }

            Row {
              id: actions
              anchors.right: parent.right
              anchors.rightMargin: Style.space(6)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(1)

              PanelActionButton {
                iconText: "\uf044"
                tooltipText: "Edit tab"
                foreground: root.fg
                fontFamily: root.fontFamily
                onClicked: root.editTab(modelData)
              }

              PanelActionButton {
                iconText: "\uf062"
                tooltipText: "Move up"
                foreground: index === 0 ? root.dimmer : root.fg
                enabled: index > 0
                fontFamily: root.fontFamily
                onClicked: root.moveTab(modelData.id, "up")
              }

              PanelActionButton {
                iconText: "\uf063"
                tooltipText: "Move down"
                foreground: index === root.tabs.length - 1 ? root.dimmer : root.fg
                enabled: index < root.tabs.length - 1
                fontFamily: root.fontFamily
                onClicked: root.moveTab(modelData.id, "down")
              }

              PanelActionButton {
                iconText: "\uf00d"
                tooltipText: root.pendingDeleteId === modelData.id ? "Click again to delete" : "Delete tab"
                foreground: root.pendingDeleteId === modelData.id ? root.urgent : root.dimmer
                fontFamily: root.fontFamily
                onClicked: root.removeTab(modelData.id)
              }
            }
          }
        }

        Item {
          visible: root.tabs.length === 0
          width: parent.width
          height: Style.space(90)

          Text {
            anchors.centerIn: parent
            width: Style.space(380)
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: "No custom tabs yet. Create Gaming, Tech, AI, World — or anything else — and assign feeds to each one."
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
          }
        }
      }
    }
  }

  // ------------------------------------------------------------------ editor

  ColumnLayout {
    anchors.fill: parent
    spacing: Style.space(6)
    visible: root.editing

    RowLayout {
      Layout.fillWidth: true
      spacing: Style.space(6)

      Text {
        textFormat: Text.PlainText
        text: "NAME"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        font.letterSpacing: 0.5
      }

      TextField {
        id: nameField
        Layout.fillWidth: true
        placeholderText: "Gaming, Tech, AI, World…"
        text: root.draftName
        foreground: root.fg
        accent: root.accent
        font.pixelSize: Style.font.bodySmall
        verticalPadding: Style.space(4)
        onTextEdited: root.draftName = text
        onAccepted: root.saveTab()
        Keys.onEscapePressed: root.cancelEdit()
      }

      Text {
        textFormat: Text.PlainText
        text: "GLYPH"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }

      TextField {
        id: glyphField
        implicitWidth: Style.space(46)
        placeholderText: "\uf11b"
        text: root.draftGlyph
        foreground: root.fg
        accent: root.accent
        font.pixelSize: Style.font.bodySmall
        verticalPadding: Style.space(4)
        maximumLength: 2
        onTextEdited: root.draftGlyph = text
        Keys.onEscapePressed: root.cancelEdit()
      }
    }

    RowLayout {
      Layout.fillWidth: true
      spacing: Style.space(6)

      Text {
        textFormat: Text.PlainText
        text: "INCLUDE"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }

      TextField {
        id: includeField
        Layout.fillWidth: true
        placeholderText: "keywords (comma separated) — article must match one"
        text: root.draftInclude
        foreground: root.fg
        accent: root.accent
        font.pixelSize: Style.font.caption
        verticalPadding: Style.space(3)
        onTextEdited: root.draftInclude = text
        Keys.onEscapePressed: root.cancelEdit()
      }
    }

    RowLayout {
      Layout.fillWidth: true
      spacing: Style.space(6)

      Text {
        textFormat: Text.PlainText
        text: "EXCLUDE"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }

      TextField {
        id: excludeField
        Layout.fillWidth: true
        placeholderText: "keywords to hide — e.g. sponsored, rumor"
        text: root.draftExclude
        foreground: root.fg
        accent: root.accent
        font.pixelSize: Style.font.caption
        verticalPadding: Style.space(3)
        onTextEdited: root.draftExclude = text
        Keys.onEscapePressed: root.cancelEdit()
      }
    }

    RowLayout {
      Layout.fillWidth: true
      spacing: Style.space(8)

      Text {
        Layout.fillWidth: true
        textFormat: Text.PlainText
        text: "Desktop notifications for new articles in this tab"
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }

      ToggleSwitch {
        checked: root.draftNotify
        foreground: root.fg
        accent: root.accent
        onToggled: root.draftNotify = !root.draftNotify
      }
    }

    RowLayout {
      Layout.fillWidth: true
      spacing: Style.space(6)

      Text {
        Layout.fillWidth: true
        textFormat: Text.PlainText
        text: "FEEDS IN THIS TAB"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        font.letterSpacing: 0.5
      }

      Text {
        textFormat: Text.PlainText
        text: "keyword-only when no feeds are selected"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }
    }

    Flickable {
      Layout.fillWidth: true
      Layout.fillHeight: true
      clip: true
      contentWidth: width
      contentHeight: feedPickColumn.implicitHeight
      boundsBehavior: Flickable.StopAtBounds
      flickableDirection: Flickable.VerticalFlick
      interactive: contentHeight > height
      ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

      Column {
        id: feedPickColumn
        width: parent.width
        spacing: Style.space(3)

        Repeater {
          model: root.feeds

          BorderSurface {
            id: pickRow
            required property var modelData

            width: feedPickColumn.width
            implicitHeight: Style.space(28)
            radius: Style.cornerRadius
            color: root.draftFeeds[modelData.id] === true
              ? Style.selectedFillFor(root.fg, root.accent)
              : (pickMouse.containsMouse ? Style.hoverFillFor(root.fg, root.accent) : "transparent")
            borderSpec: Border.controlSpec(
              root.draftFeeds[modelData.id] === true ? "selected" : "normal",
              root.fg, root.accent)

            Row {
              anchors.left: parent.left
              anchors.leftMargin: Style.space(8)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(7)

              Text {
                textFormat: Text.PlainText
                text: root.draftFeeds[modelData.id] === true ? "\uf00c" : "\uf096"
                color: root.draftFeeds[modelData.id] === true ? root.accent : root.dimmer
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
              }

              Text {
                textFormat: Text.PlainText
                text: String(modelData.title || modelData.url || "")
                color: root.fg
                font.family: root.fontFamily
                font.pixelSize: Style.font.bodySmall
              }

              Text {
                visible: modelData.enabled !== true
                textFormat: Text.PlainText
                text: "disabled"
                color: root.dimmer
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
              }
            }

            MouseArea {
              id: pickMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: root.toggleDraftFeed(modelData.id)
            }
          }
        }

        Text {
          visible: root.feeds.length === 0
          width: parent.width
          textFormat: Text.PlainText
          text: "No feeds yet — add subscriptions first."
          color: root.dimmer
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
        }
      }
    }

    Text {
      visible: root.message.length > 0
      Layout.fillWidth: true
      textFormat: Text.PlainText
      text: root.message
      color: root.messageIsError ? root.urgent : root.accent
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
    }

    RowLayout {
      Layout.fillWidth: true
      spacing: Style.space(6)

      BorderSurface {
        implicitWidth: saveLabel.implicitWidth + Style.space(18)
        implicitHeight: Style.space(28)
        radius: Style.cornerRadius
        color: saveMouse.containsMouse ? Style.selectedFillFor(root.fg, root.accent) : "transparent"
        borderSpec: Border.controlSpec("normal", root.accent, root.accent)

        Text {
          id: saveLabel
          anchors.centerIn: parent
          textFormat: Text.PlainText
          text: root.editingId.length > 0 ? "SAVE TAB" : "CREATE TAB"
          color: root.accent
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
        }

        MouseArea {
          id: saveMouse
          anchors.fill: parent
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: root.saveTab()
        }
      }

      BorderSurface {
        implicitWidth: cancelLabel.implicitWidth + Style.space(18)
        implicitHeight: Style.space(28)
        radius: Style.cornerRadius
        color: cancelMouse.containsMouse ? Style.hoverFillFor(root.fg, root.accent) : "transparent"
        borderSpec: Border.controlSpec("normal", root.fg, root.accent)

        Text {
          id: cancelLabel
          anchors.centerIn: parent
          textFormat: Text.PlainText
          text: "CANCEL"
          color: root.dim
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
        }

        MouseArea {
          id: cancelMouse
          anchors.fill: parent
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: root.cancelEdit()
        }
      }

      Item { Layout.fillWidth: true }

      Text {
        textFormat: Text.PlainText
        text: "Esc leaves the editor"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }
    }
  }
}
