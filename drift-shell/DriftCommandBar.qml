import QtQuick
import Quickshell.Io

Rectangle {
    id: root

    required property QtObject driftState
    required property color bgColor
    required property color bgSecondary
    required property color textColor
    required property color textDim
    required property color accentColor

    signal commandExecuted(string project, string command, string result)

    property var commandHistory: []
    property int historyIndex: -1

    height: 38
    color: root.bgSecondary

    // Prompt indicator
    Text {
        id: prompt
        text: "\u25b8"
        font.family: "JetBrainsMono Nerd Font"
        font.pixelSize: 12
        color: root.accentColor
        anchors.verticalCenter: parent.verticalCenter
        x: 10
    }

    // Placeholder
    Text {
        visible: commandInput.text.length === 0 && !commandInput.activeFocus
        text: "project: command or task..."
        font.family: "JetBrainsMono Nerd Font"
        font.pixelSize: 12
        color: root.textDim
        anchors.verticalCenter: parent.verticalCenter
        anchors.left: prompt.right
        anchors.leftMargin: 8
    }

    TextInput {
        id: commandInput
        anchors.verticalCenter: parent.verticalCenter
        anchors.left: prompt.right
        anchors.right: confirmText.left
        anchors.leftMargin: 8
        anchors.rightMargin: 8
        font.family: "JetBrainsMono Nerd Font"
        font.pixelSize: 12
        color: root.textColor
        clip: true

        onAccepted: parseAndExecute(text)

        Keys.onEscapePressed: {
            commandInput.text = "";
            commandInput.focus = false;
        }
        Keys.onUpPressed: {
            if (root.commandHistory.length === 0) return;
            if (root.historyIndex < 0) root.historyIndex = root.commandHistory.length;
            root.historyIndex = Math.max(0, root.historyIndex - 1);
            commandInput.text = root.commandHistory[root.historyIndex];
        }
        Keys.onDownPressed: {
            if (root.historyIndex < 0) return;
            root.historyIndex++;
            if (root.historyIndex >= root.commandHistory.length) {
                root.historyIndex = -1;
                commandInput.text = "";
            } else {
                commandInput.text = root.commandHistory[root.historyIndex];
            }
        }
    }

    // Confirmation text
    Text {
        id: confirmText
        anchors.verticalCenter: parent.verticalCenter
        anchors.right: parent.right
        anchors.rightMargin: 10
        font.family: "JetBrainsMono Nerd Font"
        font.pixelSize: 11
        color: root.textDim
        opacity: 0

        Behavior on opacity { NumberAnimation { duration: 300 } }
    }

    Timer {
        id: confirmTimer
        interval: 3000
        onTriggered: confirmText.opacity = 0
    }

    function showConfirmation(msg) {
        confirmText.text = msg;
        confirmText.opacity = 1;
        confirmTimer.restart();
    }

    function addToHistory(cmd) {
        var hist = commandHistory.slice();
        var idx = hist.indexOf(cmd);
        if (idx >= 0) hist.splice(idx, 1);
        hist.push(cmd);
        if (hist.length > 20) hist = hist.slice(-20);
        commandHistory = hist;
        historyIndex = -1;
    }

    function parseAndExecute(text) {
        text = text.trim();
        if (!text) return;

        var colonIdx = text.indexOf(':');
        var project = "";
        var command = text;

        if (colonIdx > 0) {
            project = text.substring(0, colonIdx).trim();
            command = text.substring(colonIdx + 1).trim();
        } else if (text.startsWith(':')) {
            project = "";
            command = text.substring(1).trim();
        }

        if (!command) return;

        var lowerCmd = command.toLowerCase();
        if (lowerCmd === "dispatch" || lowerCmd === "dispatch next") {
            if (project) {
                execProc.command = ["drift", "dispatch", project];
            } else {
                execProc.command = ["drift", "dispatch", "--next"];
            }
            showConfirmation("Dispatching" + (project ? " to " + project : "") + "...");
            execProc.running = true;
        } else if (project) {
            execProc.command = ["drift", "task", "add", project, command];
            showConfirmation("Task added to " + project);
            execProc.running = true;
        }

        addToHistory(text);
        commandInput.text = "";
        root.commandExecuted(project, command, "");
    }

    Process {
        id: execProc
        command: ["drift", "help"]
        onExited: (exitCode, exitStatus) => {
            if (exitCode !== 0) {
                showConfirmation("Command failed");
            }
        }
    }
}
