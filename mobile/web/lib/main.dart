import 'dart:async';
import 'dart:js_interop';
import 'package:web/web.dart' as web;
import 'websocket_client.dart';
import 'terminal_renderer.dart';

void main() {
  final app = TerminalApp();
  app.run();
}

class TerminalApp {
  late web.HTMLDivElement _container;
  late web.HTMLCanvasElement _canvas;
  late TerminalRenderer _renderer;
  late WebSocketClient _client;

  String? _relayUrl;
  String? _sessionName;

  Future<void> run() async {
    // Use existing HTML elements
    _container = web.document.getElementById('terminal') as web.HTMLDivElement;
    _canvas = web.document.getElementById('canvas') as web.HTMLCanvasElement;

    // Initialize renderer
    _renderer = TerminalRenderer(
      canvas: _canvas,
      cellWidth: 9,
      cellHeight: 18,
    );

    // Initialize WebSocket client
    _client = WebSocketClient();

    // Get relay URL from query params or use default
    final search = web.window.location.search;
    final params = search.isEmpty ? web.URLSearchParams() : web.URLSearchParams(search.toJS);
    _relayUrl = params.get('relay') ?? '100.95.116.72';
    _sessionName = params.get('session') ?? '';

    // Connect
    await _client.connect(_relayUrl!);
    print('[App] Connected');

    // Setup input handling
    _setupInput();

    // Handle screen updates - MUST be before createSession to catch initial screen
    _client.onScreen.listen((screen) {
      print('[App] onScreen: ${screen.cols}x${screen.rows}');
      _renderer.render(screen);
    });

    _client.onError.listen((error) {
      print('[Error] $error');
    });

    // Create session AFTER listener is registered
    if (_sessionName != null && _sessionName!.isNotEmpty) {
      await _client.createSession(
        name: _sessionName!,
        cols: 80,
        rows: 24,
      );
    } else {
      // Generate a session name
      final name = 'session_${DateTime.now().millisecondsSinceEpoch}';
      await _client.createSession(
        name: name,
        cols: 80,
        rows: 24,
      );
      _sessionName = name;
    }

    print('[App] Session created: $_sessionName');

    // Handle resize
    web.window.addEventListener('resize', ((web.Event event) => _handleResize()).toJS);
    _handleResize();
  }

  void _setupInput() {
    web.document.addEventListener('keydown', ((web.KeyboardEvent event) {
      final bytes = _renderer.handleKeyEvent(event);
      if (bytes != null) {
        _client.sendKeys(bytes);
        event.preventDefault();
      }
    }).toJS);

    web.document.addEventListener('keypress', ((web.KeyboardEvent event) {
      if (event.key.length == 1 && !event.ctrlKey && !event.altKey && !event.metaKey) {
        _client.sendKeys(event.key.codeUnits);
        event.preventDefault();
      }
    }).toJS);
  }

  void _handleResize() {
    final containerWidth = _container.clientWidth;
    final containerHeight = _container.clientHeight - 40; // subtract header

    final cols = (containerWidth / _renderer.cellWidth).floor().clamp(80, 200);
    final rows = (containerHeight / _renderer.cellHeight).floor().clamp(24, 100);

    _client.resize(cols, rows);
  }
}
