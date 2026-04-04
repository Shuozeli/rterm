/// Placeholder gRPC/FlatBuffers client for rterm-agent.
///
/// rterm-agent speaks gRPC with a FlatBuffers codec (not protobuf), so
/// standard Dart gRPC generated stubs do not work directly. This file
/// defines the interface and a placeholder implementation that logs calls.
///
/// Actual FlatBuffers integration (manual frame encoding or a REST/JSON
/// wrapper on the agent side) is a separate task.
library;

import 'dart:async';

/// Represents an active terminal session on the agent.
class SessionInfo {
  final String name;
  final int cols;
  final int rows;
  final Duration idleTime;

  const SessionInfo({
    required this.name,
    required this.cols,
    required this.rows,
    this.idleTime = Duration.zero,
  });

  @override
  String toString() => '$name (${cols}x$rows, idle ${idleTime.inSeconds}s)';
}

/// Client interface for communicating with rterm-agent.
///
/// The agent exposes a gRPC service with FlatBuffers encoding. For the MVP
/// scaffold, this is a placeholder that returns fake data so the UI flow
/// can be demonstrated end-to-end.
class RtermClient {
  String? _host;
  int? _port;
  bool _connected = false;

  bool get isConnected => _connected;
  String? get host => _host;
  int? get port => _port;

  /// Connect to the agent at [host]:[port].
  Future<void> connect(String host, int port) async {
    _host = host;
    _port = port;
    // TODO: establish real HTTP/2 or gRPC channel
    _connected = true;
  }

  /// Disconnect from the agent.
  Future<void> disconnect() async {
    _connected = false;
    _host = null;
    _port = null;
  }

  /// List active sessions on the agent.
  Future<List<SessionInfo>> listSessions() async {
    _ensureConnected();
    // TODO: real gRPC call with FlatBuffers codec
    return [];
  }

  /// Create a new SSH session.
  Future<SessionInfo> createSession({
    required String name,
    required String sshTarget,
    int cols = 80,
    int rows = 24,
  }) async {
    _ensureConnected();
    // TODO: real gRPC CreateSession with SSH target
    return SessionInfo(name: name, cols: cols, rows: rows);
  }

  /// Kill a session by name.
  Future<void> killSession(String name) async {
    _ensureConnected();
    // TODO: real gRPC KillSession
  }

  /// Send raw keystrokes to a session.
  Future<void> sendKeys(String session, String data) async {
    _ensureConnected();
    // TODO: real gRPC SendKeys / TypeText
  }

  /// Get a plain-text snapshot of the terminal screen.
  Future<String> getSnapshot(String session) async {
    _ensureConnected();
    // TODO: real gRPC GetSnapshot
    // Return placeholder content to demonstrate the UI
    return 'user@host:~\$ ';
  }

  /// Resize a session's terminal.
  Future<void> resizeSession(String session, int cols, int rows) async {
    _ensureConnected();
    // TODO: real gRPC ResizeSession
  }

  void _ensureConnected() {
    if (!_connected) {
      throw StateError('Not connected to agent. Call connect() first.');
    }
  }
}
