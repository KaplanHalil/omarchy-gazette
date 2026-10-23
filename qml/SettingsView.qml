import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Settings: appearance, refresh cadence, notifications and local data.
Item {
  id: root

  property var panel: null
  property var gazetteSettings: ({})
  property var stats: ({})
  property var paths: ({})
  property string version: "1.0.0"
  readonly property string coffeeUrl: "https://buymeacoffee.com/kaplanhalil"

  signal changed()
  signal refreshRequested()

  readonly property color fg: Commons.Color.foreground
  readonly property color accent: Commons.Color.accent
  readonly property color urgent: Commons.Color.urgent
  readonly property color dim: Qt.rgba(fg.r, fg.g, fg.b, 0.62)
  readonly property color dimmer: Qt.rgba(fg.r, fg.g, fg.b, 0.42)
  readonly property string fontFamily: Style.font.family
  readonly property bool formFocused: refreshField.activeFocus
    || marketField.activeFocus
    || notifyField.activeFocus
    || quietStartField.activeFocus
    || quietEndField.activeFocus

  property string actionMessage: ""

  function save(patch) {
    panel.saveSettings(patch)
  }

  function numberValue(field, fallback) {
    var value = parseInt(field.text, 10)
    return isFinite(value) && value > 0 ? value : fallback
  }

  Flickable {
    anchors.fill: parent
    clip: true
    contentWidth: width
    contentHeight: settingsColumn.implicitHeight
    boundsBehavior: Flickable.StopAtBounds
    flickableDirection: Flickable.VerticalFlick
    interactive: contentHeight > height
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

    Column {
      id: settingsColumn
      width: parent.width
      spacing: Style.space(7)

      PanelSectionHeader {
        width: parent.width
        text: "READING"
        foreground: root.fg
        fontFamily: root.fontFamily
      }

      Toggle {
        width: parent.width
        label: "Article images"
        description: "Download thumbnails and show them in the list and reader"
        checked: root.gazetteSettings.show_images !== false
        foreground: root.fg
        accent: root.accent
        fontFamily: root.fontFamily
        onClicked: root.save({ show_images: !(root.gazetteSettings.show_images !== false) })
      }

      Toggle {
        width: parent.width
        label: "Excerpts"
        description: "Two lines of the article summary under each headline"
        checked: root.gazetteSettings.show_excerpts !== false
        foreground: root.fg
        accent: root.accent
        fontFamily: root.fontFamily
        onClicked: root.save({ show_excerpts: !(root.gazetteSettings.show_excerpts !== false) })
      }

      Toggle {
        width: parent.width
        label: "Full article text"
        description: "Show a Read full text button in the reader; the article is downloaded only when clicked"
        checked: root.gazetteSettings.full_text !== false
        foreground: root.fg
        accent: root.accent
        fontFamily: root.fontFamily
        onClicked: root.save({ full_text: !(root.gazetteSettings.full_text !== false) })
      }

      RowLayout {
        width: parent.width
        spacing: Style.space(6)

        Text {
          textFormat: Text.PlainText
          text: "DENSITY"
          color: root.dimmer
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          font.letterSpacing: 0.5
        }

        Repeater {
          model: [
            { id: "comfortable", label: "Comfortable" },
            { id: "compact", label: "Compact" }
          ]

          BorderSurface {
            required property var modelData
            implicitWidth: densityLabel.implicitWidth + Style.space(16)
            implicitHeight: Style.space(24)
            radius: Style.cornerRadius
            color: root.gazetteSettings.density === modelData.id
              ? Style.selectedFillFor(root.fg, root.accent)
              : (densityMouse.containsMouse ? Style.hoverFillFor(root.fg, root.accent) : "transparent")
            borderSpec: Border.controlSpec(root.gazetteSettings.density === modelData.id ? "selected" : "normal", root.fg, root.accent)

            Text {
              id: densityLabel
              anchors.centerIn: parent
              textFormat: Text.PlainText
              text: modelData.label
              color: root.gazetteSettings.density === modelData.id ? root.accent : root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
            }

            MouseArea {
              id: densityMouse
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: root.save({ density: modelData.id })
            }
          }
        }

        Item { Layout.fillWidth: true }
      }

      PanelSeparator { width: parent.width }

      PanelSectionHeader {
        width: parent.width
        text: "REFRESH"
        foreground: root.fg
        fontFamily: root.fontFamily
      }

      RowLayout {
        width: parent.width
        spacing: Style.space(8)

        Text {
          Layout.fillWidth: true
          textFormat: Text.PlainText
          text: "Fetch feeds every"
          color: root.fg
          font.family: root.fontFamily
          font.pixelSize: Style.font.bodySmall
        }

        TextField {
          id: refreshField
          implicitWidth: Style.space(64)
          text: String(root.gazetteSettings.refresh_minutes || 5)
          foreground: root.fg
          accent: root.accent
          font.pixelSize: Style.font.bodySmall
          horizontalAlignment: TextInput.AlignRight
          verticalPadding: Style.space(3)
          onEditingFinished: root.save({ refresh_minutes: root.numberValue(refreshField, 5) })
        }

        Text {
          textFormat: Text.PlainText
          text: "minutes"
          color: root.dimmer
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
        }
      }

      RowLayout {
        width: parent.width
        spacing: Style.space(8)

        Text {
          Layout.fillWidth: true
          textFormat: Text.PlainText
          text: "Refresh prices every"
          color: root.fg
          font.family: root.fontFamily
          font.pixelSize: Style.font.bodySmall
        }

        TextField {
          id: marketField
          implicitWidth: Style.space(64)
          text: String(root.gazetteSettings.market_refresh_seconds || 60)
          foreground: root.fg
          accent: root.accent
          font.pixelSize: Style.font.bodySmall
          horizontalAlignment: TextInput.AlignRight
          verticalPadding: Style.space(3)
          onEditingFinished: root.save({ market_refresh_seconds: root.numberValue(marketField, 60) })
        }

        Text {
          textFormat: Text.PlainText
          text: "seconds"
          color: root.dimmer
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
        }
      }

      PanelSeparator { width: parent.width }

      PanelSectionHeader {
        width: parent.width
        text: "NOTIFICATIONS"
        foreground: root.fg
        fontFamily: root.fontFamily
      }

      Toggle {
        width: parent.width
        label: "Desktop notifications"
        description: "One notification per refresh when new unread articles arrive"
        checked: root.gazetteSettings.notifications !== false
        foreground: root.fg
        accent: root.accent
        fontFamily: root.fontFamily
        onClicked: root.save({ notifications: !(root.gazetteSettings.notifications !== false) })
      }

      RowLayout {
        width: parent.width
        spacing: Style.space(8)

        Text {
          Layout.fillWidth: true
          textFormat: Text.PlainText
          text: "Minimum minutes between notifications"
          color: root.fg
          font.family: root.fontFamily
          font.pixelSize: Style.font.bodySmall
        }

        TextField {
          id: notifyField
          implicitWidth: Style.space(64)
          text: String(root.gazetteSettings.notify_min_interval_minutes || 30)
          foreground: root.fg
          accent: root.accent
          font.pixelSize: Style.font.bodySmall
          horizontalAlignment: TextInput.AlignRight
          verticalPadding: Style.space(3)
          onEditingFinished: root.save({ notify_min_interval_minutes: root.numberValue(notifyField, 30) })
        }
      }

      RowLayout {
        width: parent.width
        spacing: Style.space(6)

        Text {
          textFormat: Text.PlainText
          text: "QUIET HOURS"
          color: root.dimmer
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          font.letterSpacing: 0.5
        }

        TextField {
          id: quietStartField
          implicitWidth: Style.space(58)
          text: String(root.gazetteSettings.quiet_start || "23:00")
          foreground: root.fg
          accent: root.accent
          font.pixelSize: Style.font.caption
          horizontalAlignment: TextInput.AlignHCenter
          verticalPadding: Style.space(3)
          onEditingFinished: root.save({ quiet_start: quietStartField.text })
        }

        Text {
          textFormat: Text.PlainText
          text: "to"
          color: root.dimmer
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
        }

        TextField {
          id: quietEndField
          implicitWidth: Style.space(58)
          text: String(root.gazetteSettings.quiet_end || "08:00")
          foreground: root.fg
          accent: root.accent
          font.pixelSize: Style.font.caption
          horizontalAlignment: TextInput.AlignHCenter
          verticalPadding: Style.space(3)
          onEditingFinished: root.save({ quiet_end: quietEndField.text })
        }

        Item { Layout.fillWidth: true }

        Text {
          textFormat: Text.PlainText
          text: "local time"
          color: root.dimmer
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
        }
      }

      PanelSeparator { width: parent.width }

      PanelSectionHeader {
        width: parent.width
        text: "DATA"
        foreground: root.fg
        fontFamily: root.fontFamily
      }

      RowLayout {
        width: parent.width
        spacing: Style.space(6)

        BorderSurface {
          implicitWidth: refreshNowLabel.implicitWidth + Style.space(18)
          implicitHeight: Style.space(28)
          radius: Style.cornerRadius
          color: refreshNowMouse.containsMouse ? Style.selectedFillFor(root.fg, root.accent) : "transparent"
          borderSpec: Border.controlSpec("normal", root.accent, root.accent)

          Text {
            id: refreshNowLabel
            anchors.centerIn: parent
            textFormat: Text.PlainText
            text: "REFRESH NOW"
            color: root.accent
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
            font.bold: true
          }

          MouseArea {
            id: refreshNowMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.refreshRequested()
          }
        }

        BorderSurface {
          implicitWidth: cleanLabel.implicitWidth + Style.space(18)
          implicitHeight: Style.space(28)
          radius: Style.cornerRadius
          color: cleanMouse.containsMouse ? Style.hoverFillFor(root.fg, root.accent) : "transparent"
          borderSpec: Border.controlSpec("normal", root.fg, root.accent)

          Text {
            id: cleanLabel
            anchors.centerIn: parent
            textFormat: Text.PlainText
            text: "CLEAN CACHE"
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
          }

          MouseArea {
            id: cleanMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
              root.panel.runCommand(["--clean"], function(result, code, errorText) {
                root.actionMessage = result && result.ok ? "Cache cleaned." : (errorText || "Clean failed.")
                root.changed()
              })
            }
          }
        }

        Item { Layout.fillWidth: true }
      }

      Text {
        width: parent.width
        visible: root.actionMessage.length > 0
        textFormat: Text.PlainText
        text: root.actionMessage
        color: root.accent
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        wrapMode: Text.Wrap
      }

      Text {
        width: parent.width
        textFormat: Text.PlainText
        text: {
          var lines = []
          lines.push("articles " + Number(root.stats.total || 0) + " · unread " + Number(root.stats.unread || 0) + " · feeds " + Number(root.stats.feeds || 0))
          lines.push("config  " + String(root.paths.config || ""))
          lines.push("state   " + String(root.paths.state || ""))
          lines.push("images  " + String(root.paths.images || ""))
          lines.push("Gazette " + root.version)
          return lines.join("\n")
        }
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        lineHeight: 1.35
        wrapMode: Text.WrapAnywhere
      }

      PanelSeparator { width: parent.width }

      PanelSectionHeader {
        width: parent.width
        text: "SUPPORT"
        foreground: root.fg
        fontFamily: root.fontFamily
      }

      Text {
        width: parent.width
        textFormat: Text.PlainText
        text: "Gazette is free and local-first. If it is useful to you, you can buy me a coffee."
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        wrapMode: Text.Wrap
      }

      BorderSurface {
        implicitWidth: coffeeLabel.implicitWidth + Style.space(18)
        implicitHeight: Style.space(28)
        radius: Style.cornerRadius
        color: coffeeMouse.containsMouse ? Style.selectedFillFor(root.fg, root.accent) : "transparent"
        borderSpec: Border.controlSpec("normal", root.accent, root.accent)

        Text {
          id: coffeeLabel
          anchors.centerIn: parent
          textFormat: Text.PlainText
          text: "BUY ME A COFFEE"
          color: root.accent
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
        }

        MouseArea {
          id: coffeeMouse
          anchors.fill: parent
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: Qt.openUrlExternally(root.coffeeUrl)
        }
      }

      Item { width: 1; height: Style.space(6) }
    }
  }
}
