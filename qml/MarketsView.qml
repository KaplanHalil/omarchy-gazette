import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Ticker configuration: enable/reorder presets, add any Yahoo symbol, and pick
// the quote shown in the bar.
Item {
  id: root

  property var panel: null
  property var markets: []
  property var catalog: []
  property var pulseSettings: ({})
  property real marketUpdated: 0

  signal changed()
  signal refreshRequested()

  property string draftLabel: ""
  property string draftSymbol: ""
  property string draftQuote: "TRY"
  property string message: ""
  property bool messageIsError: false

  readonly property color fg: Commons.Color.foreground
  readonly property color accent: Commons.Color.accent
  readonly property color urgent: Commons.Color.urgent
  readonly property color dim: Qt.rgba(fg.r, fg.g, fg.b, 0.62)
  readonly property color dimmer: Qt.rgba(fg.r, fg.g, fg.b, 0.42)
  readonly property string fontFamily: Style.font.family
  readonly property bool formFocused: labelField.activeFocus || symbolField.activeFocus
  readonly property int enabledCount: {
    var n = 0
    for (var i = 0; i < markets.length; i++) if (markets[i].enabled) n += 1
    return n
  }

  function focusAdd() {
    labelField.forceActiveFocus()
  }

  function clock(ts) {
    if (!ts || ts <= 0) return "—"
    var d = new Date(ts * 1000)
    var h = d.getHours()
    var m = d.getMinutes()
    return (h < 10 ? "0" : "") + h + ":" + (m < 10 ? "0" : "") + m
  }

  function clean(list) {
    var out = []
    for (var i = 0; i < list.length; i++) {
      var m = list[i]
      out.push({
        id: String(m.id || ""),
        label: String(m.label || ""),
        kind: String(m.kind || "yahoo"),
        symbol: String(m.symbol || ""),
        quote: String(m.quote || "TRY"),
        enabled: m.enabled === true,
        decimals: Number(m.decimals === undefined ? 2 : m.decimals)
      })
    }
    return out
  }

  function save(list, note) {
    panel.runCommand(["--save-markets", JSON.stringify(clean(list))], function(result, code, errorText) {
      if (!result || result.ok === false) {
        message = errorText || "Could not save tickers."
        messageIsError = true
        return
      }
      message = note || ""
      messageIsError = false
      root.changed()
      root.refreshRequested()
    })
  }

  function withChanges(mutator) {
    var list = []
    for (var i = 0; i < markets.length; i++) list.push(markets[i])
    mutator(list)
    save(list)
  }

  function toggleMarket(market) {
    withChanges(function(list) {
      for (var i = 0; i < list.length; i++) {
        if (list[i].id === market.id) list[i] = Object.assign({}, list[i], { enabled: !list[i].enabled })
      }
    })
  }

  function moveMarket(index, direction) {
    withChanges(function(list) {
      var target = direction < 0 ? index - 1 : index + 1
      if (target < 0 || target >= list.length) return
      var tmp = list[index]
      list[index] = list[target]
      list[target] = tmp
    })
  }

  function removeMarket(market) {
    withChanges(function(list) {
      for (var i = list.length - 1; i >= 0; i--) if (list[i].id === market.id) list.splice(i, 1)
    })
  }

  function hasMarket(kind, symbol) {
    for (var i = 0; i < markets.length; i++) {
      if (String(markets[i].kind) === kind && String(markets[i].symbol) === symbol) return true
    }
    return false
  }

  function addPreset(entry) {
    if (hasMarket(String(entry.kind), String(entry.symbol))) {
      message = "That ticker is already in your list."
      messageIsError = true
      return
    }
    withChanges(function(list) {
      list.push({
        id: String(entry.id || ""),
        label: String(entry.label || entry.symbol),
        kind: String(entry.kind || "yahoo"),
        symbol: String(entry.symbol || ""),
        quote: String(entry.quote || "TRY"),
        enabled: true,
        decimals: Number(entry.decimals === undefined ? 2 : entry.decimals)
      })
    })
    message = "Added " + String(entry.label || entry.symbol) + "."
    messageIsError = false
  }

  function addCustom() {
    var label = draftLabel.trim()
    var symbol = draftSymbol.trim()
    if (symbol.length === 0) {
      message = "Enter a Yahoo symbol, e.g. USDTRY=X or XAUUSD=X."
      messageIsError = true
      return
    }
    withChanges(function(list) {
      list.push({
        id: "",
        label: label.length > 0 ? label : symbol,
        kind: "yahoo",
        symbol: symbol,
        quote: draftQuote,
        enabled: true,
        decimals: symbol.indexOf("=X") >= 0 ? 4 : 2
      })
    })
    draftLabel = ""
    draftSymbol = ""
    message = "Ticker added."
    messageIsError = false
  }

  function setTicker(id) {
    panel.saveSettings({ bar_ticker_id: id })
  }

  ColumnLayout {
    anchors.fill: parent
    spacing: Style.space(6)

    RowLayout {
      Layout.fillWidth: true
      Layout.preferredHeight: Style.space(24)
      spacing: Style.space(6)

      Text {
        Layout.fillWidth: true
        textFormat: Text.PlainText
        text: root.enabledCount + " ACTIVE · UPDATED " + root.clock(root.marketUpdated)
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        font.letterSpacing: 0.5
      }

      PanelActionButton {
        iconText: "\uf021"
        tooltipText: "Refresh prices"
        foreground: root.fg
        fontFamily: root.fontFamily
        onClicked: root.refreshRequested()
      }
    }

    RowLayout {
      Layout.fillWidth: true
      spacing: Style.space(8)

      Text {
        Layout.fillWidth: true
        textFormat: Text.PlainText
        text: "Show one ticker in the bar"
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }

      ToggleSwitch {
        checked: root.pulseSettings.bar_ticker !== false
        foreground: root.fg
        accent: root.accent
        onToggled: root.panel.saveSettings({ bar_ticker: !(root.pulseSettings.bar_ticker !== false) })
      }
    }

    RowLayout {
      Layout.fillWidth: true
      Layout.preferredHeight: Style.space(24)
      visible: root.pulseSettings.bar_ticker !== false
      spacing: Style.space(4)

      Text {
        textFormat: Text.PlainText
        text: "BAR TICKER"
        color: root.dimmer
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        font.letterSpacing: 0.4
      }

      Repeater {
        model: root.markets

        BorderSurface {
          required property var modelData
          visible: modelData.enabled === true
          implicitWidth: barLabel.implicitWidth + Style.space(12)
          implicitHeight: Style.space(22)
          radius: Style.cornerRadius
          color: root.pulseSettings.bar_ticker_id === modelData.id
            ? Style.selectedFillFor(root.fg, root.accent)
            : (barMouse.containsMouse ? Style.hoverFillFor(root.fg, root.accent) : "transparent")
          borderSpec: Border.controlSpec(
            root.pulseSettings.bar_ticker_id === modelData.id ? "selected" : "normal",
            root.fg, root.accent)

          Text {
            id: barLabel
            anchors.centerIn: parent
            textFormat: Text.PlainText
            text: String(modelData.label || modelData.id)
            color: root.pulseSettings.bar_ticker_id === modelData.id ? root.accent : root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
          }

          MouseArea {
            id: barMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.setTicker(modelData.id)
          }
        }
      }
    }

    PanelSeparator { Layout.fillWidth: true }

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

    Flickable {
      id: marketList
      Layout.fillWidth: true
      Layout.fillHeight: true
      clip: true
      contentWidth: width
      contentHeight: marketColumn.implicitHeight
      boundsBehavior: Flickable.StopAtBounds
      flickableDirection: Flickable.VerticalFlick
      interactive: contentHeight > height
      ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

      Column {
        id: marketColumn
        width: marketList.width
        spacing: Style.space(4)

        Repeater {
          model: root.markets

          BorderSurface {
            id: marketRow
            required property var modelData
            required property int index

            width: marketColumn.width
            implicitHeight: Style.space(44)
            radius: Style.cornerRadius
            color: marketMouse.containsMouse ? Style.hoverFillFor(root.fg, root.accent) : "transparent"
            borderSpec: Border.controlSpec("normal", root.fg, root.accent)
            opacity: modelData.enabled ? 1.0 : 0.6

            MouseArea {
              id: marketMouse
              anchors.fill: parent
              hoverEnabled: true
              acceptedButtons: Qt.NoButton
            }

            Column {
              anchors.left: parent.left
              anchors.leftMargin: Style.space(9)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(1)

              Row {
                spacing: Style.space(7)

                Text {
                  textFormat: Text.PlainText
                  text: String(modelData.label || "")
                  color: root.fg
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.bodySmall
                  font.bold: true
                }

                Text {
                  textFormat: Text.PlainText
                  text: String(modelData.priceText || "—")
                  color: modelData.stale ? root.dimmer : root.fg
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.caption
                }

                Text {
                  textFormat: Text.PlainText
                  text: String(modelData.changeText || "")
                  color: Number(modelData.changePct) > 0.005
                    ? root.accent
                    : (Number(modelData.changePct) < -0.005 ? root.urgent : root.dimmer)
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.caption
                }
              }

              Text {
                textFormat: Text.PlainText
                text: String(modelData.symbol || "") + "  ·  " + String(modelData.quote || "TRY")
                color: root.dimmer
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
              }
            }

            Row {
              anchors.right: parent.right
              anchors.rightMargin: Style.space(6)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(1)

              ToggleSwitch {
                anchors.verticalCenter: parent.verticalCenter
                checked: modelData.enabled === true
                foreground: root.fg
                accent: root.accent
                onToggled: root.toggleMarket(modelData)
              }

              PanelActionButton {
                iconText: "\uf062"
                tooltipText: "Move up"
                enabled: index > 0
                foreground: index > 0 ? root.fg : root.dimmer
                fontFamily: root.fontFamily
                onClicked: root.moveMarket(index, -1)
              }

              PanelActionButton {
                iconText: "\uf063"
                tooltipText: "Move down"
                enabled: index < root.markets.length - 1
                foreground: index < root.markets.length - 1 ? root.fg : root.dimmer
                fontFamily: root.fontFamily
                onClicked: root.moveMarket(index, 1)
              }

              PanelActionButton {
                iconText: "\uf00d"
                tooltipText: "Remove ticker"
                foreground: root.dimmer
                fontFamily: root.fontFamily
                onClicked: root.removeMarket(modelData)
              }
            }
          }
        }
      }
    }

    PanelSeparator { Layout.fillWidth: true }

    Text {
      Layout.fillWidth: true
      textFormat: Text.PlainText
      text: "ADD PRESET"
      color: root.dimmer
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
      font.letterSpacing: 0.5
    }

    Flow {
      Layout.fillWidth: true
      spacing: Style.space(4)

      Repeater {
        model: root.catalog

        BorderSurface {
          required property var modelData
          visible: !root.hasMarket(String(modelData.kind), String(modelData.symbol))
          implicitWidth: presetLabel.implicitWidth + Style.space(14)
          implicitHeight: Style.space(22)
          radius: Style.cornerRadius
          color: presetMouse.containsMouse ? Style.hoverFillFor(root.fg, root.accent) : "transparent"
          borderSpec: Border.controlSpec("normal", root.fg, root.accent)

          Text {
            id: presetLabel
            anchors.centerIn: parent
            textFormat: Text.PlainText
            text: "+ " + String(modelData.label || modelData.symbol)
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
          }

          MouseArea {
            id: presetMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.addPreset(modelData)
          }
        }
      }
    }

    RowLayout {
      Layout.fillWidth: true
      spacing: Style.space(5)

      TextField {
        id: labelField
        Layout.preferredWidth: Style.space(110)
        placeholderText: "Label"
        text: root.draftLabel
        foreground: root.fg
        accent: root.accent
        font.pixelSize: Style.font.caption
        verticalPadding: Style.space(3)
        onTextEdited: root.draftLabel = text
        onAccepted: root.addCustom()
        Keys.onEscapePressed: if (root.panel) root.panel.focusList()
      }

      TextField {
        id: symbolField
        Layout.fillWidth: true
        placeholderText: "Yahoo symbol — e.g. XAUUSD=X, ^IXIC, BTC-USD"
        text: root.draftSymbol
        foreground: root.fg
        accent: root.accent
        font.pixelSize: Style.font.caption
        verticalPadding: Style.space(3)
        onTextEdited: root.draftSymbol = text
        onAccepted: root.addCustom()
        Keys.onEscapePressed: if (root.panel) root.panel.focusList()
      }

      Repeater {
        model: ["TRY", "USD"]

        BorderSurface {
          required property string modelData
          implicitWidth: quoteLabel.implicitWidth + Style.space(12)
          implicitHeight: Style.space(24)
          radius: Style.cornerRadius
          color: root.draftQuote === modelData
            ? Style.selectedFillFor(root.fg, root.accent)
            : (quoteMouse.containsMouse ? Style.hoverFillFor(root.fg, root.accent) : "transparent")
          borderSpec: Border.controlSpec(root.draftQuote === modelData ? "selected" : "normal", root.fg, root.accent)

          Text {
            id: quoteLabel
            anchors.centerIn: parent
            textFormat: Text.PlainText
            text: modelData
            color: root.draftQuote === modelData ? root.accent : root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.caption
          }

          MouseArea {
            id: quoteMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.draftQuote = modelData
          }
        }
      }

      PanelActionButton {
        iconText: "\uf067"
        tooltipText: "Add custom ticker"
        foreground: root.accent
        fontFamily: root.fontFamily
        onClicked: root.addCustom()
      }
    }
  }
}
