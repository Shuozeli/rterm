import 'package:flutter/material.dart';
import 'package:webview_flutter/webview_flutter.dart';
import '../models/host_profile.dart';
import '../services/host_storage.dart';

/// Terminal screen that hosts rterm-wasm (egui) in a WebView.
///
/// The WebView loads the rterm-wasm HTML from the relay server, which then
/// connects via WebTransport on the same relay for the SSH session.
class TerminalScreen extends StatefulWidget {
  final HostProfile host;

  const TerminalScreen({super.key, required this.host});

  @override
  State<TerminalScreen> createState() => _TerminalScreenState();
}

class _TerminalScreenState extends State<TerminalScreen> {
  late final WebViewController _controller;
  bool _loading = true;
  String? _error;
  String? _relayUrl;

  @override
  void initState() {
    super.initState();
    _loadRelayUrl();
    _initWebView();
  }

  Future<void> _loadRelayUrl() async {
    // Prefer per-host relay URL; fall back to global settings.
    if (widget.host.relayUrl != null && widget.host.relayUrl!.isNotEmpty) {
      setState(() => _relayUrl = widget.host.relayUrl);
      return;
    }
    final storage = HostStorage();
    final settings = await storage.loadSettings();
    setState(() {
      _relayUrl = settings['relay_url'] ?? 'https://localhost:4433';
    });
  }

  void _initWebView() {
    _controller = WebViewController()
      ..setJavaScriptMode(JavaScriptMode.unrestricted)
      ..setBackgroundColor(Colors.black)
      ..setNavigationDelegate(
        NavigationDelegate(
          onPageStarted: (url) {
            setState(() => _loading = true);
          },
          onPageFinished: (url) {
            setState(() => _loading = false);
          },
          onWebResourceError: (error) {
            setState(() {
              _loading = false;
              _error = error.description;
            });
          },
          onNavigationRequest: (req) {
            // Allow all navigation to relay server.
            return NavigationDecision.navigate;
          },
        ),
      );
  }

  void _loadTerminal() {
    if (_relayUrl == null || _relayUrl!.isEmpty) {
      setState(() {
        _loading = false;
        _error = 'Relay URL not configured. Set it in Settings.';
      });
      return;
    }

    // rterm-wasm reads session name from URL path.
    // e.g. https://relay:4433/my-session -> session = "my-session"
    final sessionName = _sanitizeSessionName(widget.host.name);
    final url = '$_relayUrl/$sessionName';
    setState(() => _loading = true);
    _controller.loadRequest(Uri.parse(url));
  }

  /// Sanitize session name for use in URL path.
  String _sanitizeSessionName(String name) {
    return name.replaceAll(RegExp(r'[^a-zA-Z0-9_-]'), '-');
  }

  Future<void> _disconnect() async {
    // Tell the WASM app to disconnect via JS, then pop.
    try {
      await _controller.runJavaScript('window.__rterm_disconnect?.()');
    } catch (_) {}
    if (mounted) Navigator.pop(context);
  }

  @override
  Widget build(BuildContext context) {
    if (_relayUrl == null) {
      return Scaffold(
        appBar: AppBar(title: Text(widget.host.name)),
        body: const Center(child: CircularProgressIndicator()),
      );
    }

    return Scaffold(
      appBar: AppBar(
        title: Text(widget.host.name),
        leading: IconButton(
          icon: const Icon(Icons.arrow_back),
          onPressed: _disconnect,
        ),
        actions: [
          if (_relayUrl != null)
            Padding(
              padding: const EdgeInsets.only(right: 8),
              child: Center(
                child: Text(
                  _relayUrl!,
                  style: TextStyle(
                    fontSize: 12,
                    color: Theme.of(context).colorScheme.outline,
                  ),
                ),
              ),
            ),
          IconButton(
            icon: const Icon(Icons.refresh),
            tooltip: 'Reload',
            onPressed: () {
              setState(() => _loading = true);
              _loadTerminal();
            },
          ),
        ],
      ),
      body: _error != null
          ? Center(
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
                      'Failed to load terminal',
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
                          _loading = true;
                          _error = null;
                        });
                        _loadTerminal();
                      },
                      child: const Text('Retry'),
                    ),
                  ],
                ),
              ),
            )
          : Stack(
              children: [
                WebViewWidget(controller: _controller),
                if (_loading)
                  Container(
                    color: Colors.black.withValues(alpha: 0.7),
                    child: Center(
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          const CircularProgressIndicator(
                            color: Colors.white,
                          ),
                          const SizedBox(height: 16),
                          Text(
                            'Loading rterm...',
                            style: Theme.of(context)
                                .textTheme
                                .bodyMedium
                                ?.copyWith(color: Colors.white),
                          ),
                          const SizedBox(height: 8),
                          Text(
                            '$_relayUrl/${_sanitizeSessionName(widget.host.name)}',
                            style: Theme.of(context)
                                .textTheme
                                .bodySmall
                                ?.copyWith(color: Colors.white54),
                          ),
                        ],
                      ),
                    ),
                  ),
              ],
            ),
    );
  }
}
