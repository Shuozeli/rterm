/// WebSocket client for rterm-relay.
///
/// Uses WebSocket channel to connect to the relay's WebSocket endpoint.
/// Fallback transport when gRPC H2 is unavailable.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/foundation.dart';

import 'package:flat_buffers/flat_buffers.dart' as fb;

import '../generated/rterm_rterm.protocol_generated.dart';

/// Represents an active terminal session on the relay.
class RelaySessionInfo {
  final String name;
  final String? sessionId;
  final int cols;
  final int rows;
  final Duration idleTime;

  const RelaySessionInfo({
    required this.name,
    this.sessionId,
    required this.cols,
    required this.rows,
    this.idleTime = Duration.zero,
  });

  @override
  String toString() => '$name (${cols}x$rows)';
}

/// Callback for receiving screen updates.
typedef ScreenUpdateCallback = void Function(ScreenUpdate update);
/// Callback for receiving snapshots.
typedef SnapshotCallback = void Function(ScreenSnapshot snapshot);
/// Callback for receiving session events.
typedef SessionEventCallback = void Function(int eventType);

/// Client for communicating with rterm-relay via WebSocket/FlatBuffers.
class WsClient {
  String? _host;
  int? _port;
  bool _connected = false;
  WebSocket? _socket;
  StreamSubscription? _subscription;
  String? _lastError;

  // Callbacks
  ScreenUpdateCallback? onScreenUpdate;
  SnapshotCallback? onSnapshot;
  SessionEventCallback? onSessionEvent;
  void Function(String error)? onError;

  // Pending response futures
  final _pendingResponses = <int, Completer<ServerMessage>>{};

  bool get isConnected => _connected;
  String? get host => _host;
  int? get port => _port;
  String? get lastError => _lastError;

  /// Connect to the relay WebSocket at [host]:[port] (default: 4435).
  Future<void> connect(String host, {int port = 4435}) async {
    _host = host;
    _port = port;

    final uri = 'ws://$host:$port/ws';
    debugPrint('[WsClient] Starting connection to $uri');

    try {
      // First, test TCP connectivity with a short timeout
      debugPrint('[WsClient] Testing TCP connectivity to $host:$port');
      try {
        await Socket.connect(host, port).timeout(const Duration(seconds: 3));
        debugPrint('[WsClient] TCP connection successful');
      } catch (e) {
        debugPrint('[WsClient] TCP connection failed: $e');
        throw Exception('Cannot reach relay at $host:$port - $e');
      }

      // Now try WebSocket with timeout
      debugPrint('[WsClient] Attempting WebSocket connection');
      _socket = await WebSocket.connect(uri).timeout(const Duration(seconds: 10));
      debugPrint('[WsClient] WebSocket connected!');
      _socket!.listen(
        _handleMessage,
        onError: _handleError,
        onDone: _handleDone,
      );
      _connected = true;
      debugPrint('[WsClient] _connected = true');
    } on TimeoutException {
      debugPrint('[WsClient] TimeoutException');
      _lastError = 'Connection timed out';
      _connected = false;
      throw Exception(_lastError);
    } catch (e, stack) {
      debugPrint('[WsClient] Connection error: $e\n$stack');
      _lastError = e.toString();
      _connected = false;
      rethrow;
    }
  }

  /// Disconnect from the relay.
  Future<void> disconnect() async {
    await _subscription?.cancel();
    _socket?.close();
    _socket = null;
    _subscription = null;
    _connected = false;
    _host = null;
    _port = null;
    _pendingResponses.clear();
  }

  void _handleMessage(dynamic data) {
    debugPrint('[WsClient] _handleMessage called, data type: ${data.runtimeType}');
    try {
      final bytes = data is String ? utf8.encode(data) : data as List<int>;
      debugPrint('[WsClient] Received ${bytes.length} bytes');
      final serverMsg = ServerMessage(bytes);

      // Handle different message types
      final bodyType = serverMsg.bodyType;
      debugPrint('[WsClient] bodyType: $bodyType');
      if (bodyType == null) return;

      switch (bodyType.value) {
        case 1: // ScreenUpdate
          final update = serverMsg.body as ScreenUpdate;
          onScreenUpdate?.call(update);
          break;
        case 2: // ScreenSnapshot
          final snapshot = serverMsg.body as ScreenSnapshot;
          onSnapshot?.call(snapshot);
          break;
        case 6: // SessionCreated
          onSessionEvent?.call(6);
          break;
        case 7: // SessionAttached
          onSessionEvent?.call(7);
          break;
        case 8: // SessionDetached
          onSessionEvent?.call(8);
          break;
        case 9: // SessionDestroyed
          onSessionEvent?.call(9);
          break;
        default:
          // Other message types - could be a response to a request
          break;
      }
    } catch (e) {
      // Handle parsing errors
      _lastError = e.toString();
      onError?.call(_lastError!);
    }
  }

  void _handleError(dynamic error) {
    _connected = false;
    _lastError = error?.toString() ?? 'Unknown error';
    onError?.call(_lastError!);
  }

  void _handleDone() {
    _connected = false;
  }

  Future<void> _sendMessage(ClientMessageT message) async {
    if (_socket == null) {
      throw StateError('Not connected to relay. Call connect() first.');
    }

    final builder = fb.Builder(deduplicateTables: false);
    final offset = ClientMessage.pack(builder, message);
    builder.finish(offset);

    _socket!.add(builder.buffer);
  }

  /// Create a new session.
  Future<RelaySessionInfo> createSession({
    required String name,
    String shell = '/bin/bash',
    int cols = 80,
    int rows = 24,
  }) async {
    final createSessionBody = CreateSessionT(
      name: name,
      shell: shell,
      cols: cols,
      rows: rows,
    );

    final clientMessage = ClientMessageT(
      bodyType: ClientBodyTypeId.CreateSession,
      body: createSessionBody,
    );

    await _sendMessage(clientMessage);

    return RelaySessionInfo(
      name: name,
      cols: cols,
      rows: rows,
    );
  }

  /// Kill a session by ID.
  Future<void> killSession(String sessionId) async {
    final destroyBody = DestroySessionT(sessionId: sessionId);

    final clientMessage = ClientMessageT(
      bodyType: ClientBodyTypeId.DestroySession,
      body: destroyBody,
    );

    await _sendMessage(clientMessage);
  }

  /// Send keystrokes to a session.
  Future<void> sendKeys(String session, List<int> data) async {
    final keyInput = KeyInputT(data: data);

    final clientMessage = ClientMessageT(
      bodyType: ClientBodyTypeId.KeyInput,
      body: keyInput,
    );

    await _sendMessage(clientMessage);
  }

  /// Resize a session's terminal.
  Future<void> resizeSession(String session, int cols, int rows) async {
    final resizeBody = ResizeT(cols: cols, rows: rows);

    final clientMessage = ClientMessageT(
      bodyType: ClientBodyTypeId.Resize,
      body: resizeBody,
    );

    await _sendMessage(clientMessage);
  }

  /// Attach to an existing session.
  Future<void> attachSession(String sessionId) async {
    final attachBody = AttachSessionT(sessionId: sessionId);

    final clientMessage = ClientMessageT(
      bodyType: ClientBodyTypeId.AttachSession,
      body: attachBody,
    );

    await _sendMessage(clientMessage);
  }

  /// Detach from a session.
  Future<void> detachSession() async {
    final detachBody = DetachSessionT();

    final clientMessage = ClientMessageT(
      bodyType: ClientBodyTypeId.DetachSession,
      body: detachBody,
    );

    await _sendMessage(clientMessage);
  }
}
