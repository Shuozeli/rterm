/// gRPC/FlatBuffers client for rterm-relay.
///
/// Uses the generated TerminalServiceClient with FlatBuffers codec.
/// Connect to rterm-relay on port 4434 (gRPC H2).
library;

import 'dart:async';
import 'dart:io';

import 'package:flat_buffers/flat_buffers.dart' as fb;
import 'package:grpc/grpc.dart' as grpc;

import '../generated/rterm_rterm.protocol_generated.dart';
import '../generated/terminalservice_client.dart';

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

/// Client for communicating with rterm-relay via gRPC/FlatBuffers.
class RtermClient {
  String? _host;
  int? _port;
  bool _connected = false;
  TerminalServiceClient? _client;
  grpc.ClientChannel? _channel;

  bool get isConnected => _connected;
  String? get host => _host;
  int? get port => _port;

  /// Connect to the relay at [host]:[port] (default: 4434 for gRPC H2).
  Future<void> connect(String host, {int port = 4434}) async {
    _host = host;
    _port = port;

    _channel = grpc.ClientChannel(
      InternetAddress(host, type: InternetAddressType.IPv4),
      port: port,
      options: const grpc.ChannelOptions(
        credentials: grpc.ChannelCredentials.insecure(),
      ),
    );

    _client = TerminalServiceClient(
      _channel!,
      options: grpc.CallOptions(timeout: const Duration(seconds: 30)),
    );

    _connected = true;
  }

  /// Disconnect from the relay.
  Future<void> disconnect() async {
    if (_channel != null) {
      await _channel!.shutdown();
      _channel = null;
    }
    _client = null;
    _connected = false;
    _host = null;
    _port = null;
  }

  /// List active sessions on the relay.
  Future<List<RelaySessionInfo>> listSessions() async {
    _ensureConnected();

    // Build empty request
    final builder = fb.Builder(deduplicateTables: false);
    final requestOffset = UnaryListSessionsRequest.pack(builder, UnaryListSessionsRequestT());
    builder.finish(requestOffset);
    final request = UnaryListSessionsRequest(builder.buffer);

    final response = await _client!.list_active_sessions(request);

    return response.sessions?.map((s) {
      return RelaySessionInfo(
        name: s.name ?? 'unnamed',
        sessionId: s.sessionId,
        cols: s.cols,
        rows: s.rows,
      );
    }).toList() ?? [];
  }

  /// Create a new session.
  Future<RelaySessionInfo> createSession({
    required String name,
    String shell = '/bin/bash',
    int cols = 80,
    int rows = 24,
  }) async {
    _ensureConnected();

    // Build CreateSession body
    final createSessionBody = CreateSessionT(
      name: name,
      shell: shell,
      cols: cols,
      rows: rows,
    );

    // Build ClientMessage with CreateSession body
    final clientMessage = ClientMessageT(
      bodyType: ClientBodyTypeId.CreateSession,
      body: createSessionBody,
    );

    // Serialize using FlatBuffers
    final builder = fb.Builder(deduplicateTables: false);
    final msgOffset = ClientMessage.pack(builder, clientMessage);
    builder.finish(msgOffset);
    final request = ClientMessage(builder.buffer);

    await _client!.session(request);

    return RelaySessionInfo(
      name: name,
      cols: cols,
      rows: rows,
    );
  }

  /// Kill a session by ID.
  Future<void> killSession(String sessionId) async {
    _ensureConnected();

    // Build DestroySession body
    final destroyBody = DestroySessionT(sessionId: sessionId);

    final clientMessage = ClientMessageT(
      bodyType: ClientBodyTypeId.DestroySession,
      body: destroyBody,
    );

    final builder = fb.Builder(deduplicateTables: false);
    final msgOffset = ClientMessage.pack(builder, clientMessage);
    builder.finish(msgOffset);
    final request = ClientMessage(builder.buffer);

    await _client!.session(request);
  }

  /// Send keystrokes to a session.
  Future<void> sendKeys(String session, List<int> data) async {
    _ensureConnected();

    // Build KeyInput body
    final keyInput = KeyInputT(data: data);

    final clientMessage = ClientMessageT(
      bodyType: ClientBodyTypeId.KeyInput,
      body: keyInput,
    );

    final builder = fb.Builder(deduplicateTables: false);
    final msgOffset = ClientMessage.pack(builder, clientMessage);
    builder.finish(msgOffset);
    final request = ClientMessage(builder.buffer);

    await _client!.session(request);
  }

  /// Resize a session's terminal.
  Future<void> resizeSession(String session, int cols, int rows) async {
    _ensureConnected();

    // Build Resize body
    final resizeBody = ResizeT(cols: cols, rows: rows);

    final clientMessage = ClientMessageT(
      bodyType: ClientBodyTypeId.Resize,
      body: resizeBody,
    );

    final builder = fb.Builder(deduplicateTables: false);
    final msgOffset = ClientMessage.pack(builder, clientMessage);
    builder.finish(msgOffset);
    final request = ClientMessage(builder.buffer);

    await _client!.session(request);
  }

  /// Get a full screen snapshot for a session.
  Future<GetSnapshotResponse> getSnapshot(String session) async {
    _ensureConnected();

    // Build GetSnapshotRequest using FlatBuffers T class
    final requestT = GetSnapshotRequestT(sessionName: session);
    final builder = fb.Builder(deduplicateTables: false);
    final requestOffset = GetSnapshotRequest.pack(builder, requestT);
    builder.finish(requestOffset);
    final request = GetSnapshotRequest(builder.buffer);

    return await _client!.get_snapshot(request);
  }

  void _ensureConnected() {
    if (!_connected || _client == null) {
      throw StateError('Not connected to relay. Call connect() first.');
    }
  }
}
