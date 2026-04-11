import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import '../models/host_profile.dart';
import '../models/screen_buffer.dart';
import '../services/host_storage.dart';
import '../services/websocket_client.dart';
import '../utils/screen_converter.dart';
import '../widgets/terminal_grid.dart';

/// Terminal screen with native Flutter rendering via WebSocket.
class TerminalScreen extends StatefulWidget {
  final HostProfile host;

  const TerminalScreen({super.key, required this.host});

  @override
  State<TerminalScreen> createState() => _TerminalScreenState();
}

class _TerminalScreenState extends State<TerminalScreen> {
  final _client = WsClient();
  ScreenBuffer? _screen;
  String? _error;
  bool _connecting = true;
  String? _relayUrl;
  String? _currentSessionId;
  final _focusNode = FocusNode();

  @override
  void initState() {
    super.initState();
    _focusNode.requestFocus();
    _loadRelayUrl();
  }

  @override
  void dispose() {
    _client.disconnect();
    _focusNode.dispose();
    super.dispose();
  }

  Future<void> _loadRelayUrl() async {
    debugPrint('[_loadRelayUrl] Starting');
    if (widget.host.relayUrl != null && widget.host.relayUrl!.isNotEmpty) {
      debugPrint('[_loadRelayUrl] Using host.relayUrl: ${widget.host.relayUrl}');
      setState(() => _relayUrl = widget.host.relayUrl);
    } else {
      final storage = HostStorage();
      final settings = await storage.loadSettings();
      final url = settings['relay_url'] ?? '10.0.0.150';
      debugPrint('[_loadRelayUrl] Loaded settings, relay_url: $url');
      setState(() {
        _relayUrl = url;
      });
    }
    debugPrint('[_loadRelayUrl] Calling _connect() with relayUrl: $_relayUrl');
    _connect();
  }

  Future<void> _connect() async {
    debugPrint('[_connect] Starting with relayUrl: $_relayUrl');
    if (_relayUrl == null || _relayUrl!.trim().isEmpty) {
      debugPrint('[_connect] relayUrl is null or empty!');
      setState(() {
        _connecting = false;
        _error = 'Relay URL not configured';
      });
      return;
    }

    try {
      // Extract host from relay URL
      final relayUrlTrimmed = _relayUrl!.trim();
      final uri = Uri.tryParse(relayUrlTrimmed);
      final host = (uri != null && uri.host.isNotEmpty) ? uri.host : relayUrlTrimmed;
      debugPrint('[_connect] Parsed host: "$host" from relayUrl: "$relayUrlTrimmed"');
      if (host.isEmpty) {
        throw Exception('Failed to extract valid host from relay URL');
      }

      // Connect to WebSocket (port 4435)
      await _client.connect(host, port: 4435);
      debugPrint('[_connect] client.connect completed!');

      // Set up callbacks for screen updates
      _client.onScreenUpdate = (update) {
        if (_screen != null) {
          applyScreenUpdate(_screen!, update);
          if (mounted) setState(() {});
        }
      };

      _client.onSnapshot = (snapshot) {
        debugPrint('[TerminalScreen] onSnapshot called, cols: ${snapshot.cols}, rows: ${snapshot.numRows}');
        final screen = screenBufferFromSnapshot(snapshot);
        debugPrint('[TerminalScreen] ScreenBuffer created, cols: ${screen.cols}, rows: ${screen.rows}');
        if (mounted) {
          setState(() {
            _screen = screen;
            _connecting = false;
          });
          debugPrint('[TerminalScreen] State updated');
        }
      };

      _client.onError = (error) {
        if (mounted) {
          setState(() {
            _error = error;
          });
        }
      };

      // Send Resize first (required by relay protocol)
      await _client.resizeSession('', 80, 24);

      // Create a new session
      final session = await _client.createSession(
        name: widget.host.name,
        shell: '/bin/bash',
        cols: 80,
        rows: 24,
      );

      _currentSessionId = session.name;

      // Initial screen will come via onSnapshot callback
      setState(() {
        _screen = ScreenBuffer.empty(80, 24);
        _connecting = false;
      });
    } catch (e, stack) {
      debugPrint('[_connect] Exception: $e\n$stack');
      setState(() {
        _connecting = false;
        _error = 'Connection failed: $e';
      });
    }
  }

  void _handleKey(KeyEvent event) {
    if (_currentSessionId == null) return;

    final keyData = _keyEventToBytes(event);
    if (keyData != null) {
      _client.sendKeys(_currentSessionId!, keyData);
    }
  }

  List<int>? _keyEventToBytes(KeyEvent event) {
    // Basic key mapping - simplified for now
    if (event is KeyDownEvent || event is KeyRepeatEvent) {
      final key = event.logicalKey;

      // Function keys
      if (key == LogicalKeyboardKey.enter) return [13]; // CR
      if (key == LogicalKeyboardKey.backspace) return [127]; // DEL
      if (key == LogicalKeyboardKey.tab) return [9]; // TAB
      if (key == LogicalKeyboardKey.escape) return [27]; // ESC

      // Arrow keys (VT100)
      if (key == LogicalKeyboardKey.arrowUp) return [27, 91, 65]; // ESC [ A
      if (key == LogicalKeyboardKey.arrowDown) return [27, 91, 66]; // ESC [ B
      if (key == LogicalKeyboardKey.arrowRight) return [27, 91, 67]; // ESC [ C
      if (key == LogicalKeyboardKey.arrowLeft) return [27, 91, 68]; // ESC [ D

      // Home/End
      if (key == LogicalKeyboardKey.home) return [27, 91, 72]; // ESC [ H
      if (key == LogicalKeyboardKey.end) return [27, 91, 70]; // ESC [ F

      // Delete
      if (key == LogicalKeyboardKey.delete) return [27, 91, 51, 126]; // ESC [ 3 ~

      // Regular character keys
      final char = event.character;
      if (char != null && char.isNotEmpty) {
        return char.codeUnits;
      }
    }
    return null;
  }

  void _handleTap(int row, int col) {
    // Focus for keyboard input
    FocusScope.of(context).requestFocus(FocusNode());
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.host.name),
        actions: [
          if (_relayUrl != null)
            Padding(
              padding: const EdgeInsets.only(right: 8),
              child: Center(
                child: Text(
                  '$_relayUrl:4435',
                  style: TextStyle(
                    fontSize: 12,
                    color: Theme.of(context).colorScheme.outline,
                  ),
                ),
              ),
            ),
        ],
      ),
      body: _buildBody(),
    );
  }

  Widget _buildBody() {
    if (_connecting) {
      return const Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            CircularProgressIndicator(),
            SizedBox(height: 16),
            Text('Connecting to relay...'),
          ],
        ),
      );
    }

    if (_error != null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                Icons.error_outline,
                size: 48,
                color: Theme.of(context).colorScheme.error,
              ),
              const SizedBox(height: 16),
              Text(
                'Connection failed',
                style: Theme.of(context).textTheme.titleMedium,
              ),
              const SizedBox(height: 8),
              Text(
                _error!,
                textAlign: TextAlign.center,
                style: Theme.of(context).textTheme.bodySmall?.copyWith(
                      color: Theme.of(context).colorScheme.error,
                    ),
              ),
              const SizedBox(height: 24),
              FilledButton(
                onPressed: () {
                  setState(() {
                    _connecting = true;
                    _error = null;
                  });
                  _connect();
                },
                child: const Text('Retry'),
              ),
            ],
          ),
        ),
      );
    }

    if (_screen == null) {
      return const Center(child: Text('No screen data'));
    }

    return KeyboardListener(
      focusNode: _focusNode,
      onKeyEvent: _handleKey,
      child: TerminalGrid(
        screen: _screen!,
        onTap: _handleTap,
      ),
    );
  }
}
