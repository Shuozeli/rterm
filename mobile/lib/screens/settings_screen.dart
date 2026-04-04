import 'package:flutter/material.dart';
import '../services/host_storage.dart';

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key});

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  final _relayUrlCtl = TextEditingController();
  final _storage = HostStorage();
  bool _loaded = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final settings = await _storage.loadSettings();
    setState(() {
      _relayUrlCtl.text = settings['relay_url'] ?? 'https://localhost:4433';
      _loaded = true;
    });
  }

  Future<void> _saveRelayUrl() async {
    final settings = await _storage.loadSettings();
    settings['relay_url'] = _relayUrlCtl.text.trim();
    await _storage.saveSettings(settings);
    if (mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Relay URL saved')),
      );
    }
  }

  @override
  void dispose() {
    _relayUrlCtl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Settings'),
      ),
      body: !_loaded
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              children: [
                _sectionHeader(context, 'Relay Server'),
                ListTile(
                  title: const Text('Relay URL'),
                  subtitle: Text(
                    _relayUrlCtl.text.isEmpty
                        ? 'Not configured'
                        : _relayUrlCtl.text,
                  ),
                  leading: const Icon(Icons.cloud),
                  onTap: () => _editRelayUrl(),
                ),
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  child: Text(
                    'The relay server URL (e.g. https://relay.example.com:4433). '
                    'rterm-wasm loads from this server and connects via WebTransport.',
                    style: Theme.of(context).textTheme.bodySmall?.copyWith(
                          color: Theme.of(context).colorScheme.outline,
                        ),
                  ),
                ),
                const SizedBox(height: 8),
                ListTile(
                  title: const Text('Save'),
                  leading: const Icon(Icons.save),
                  onTap: _saveRelayUrl,
                ),
                const Divider(),
                _sectionHeader(context, 'Appearance'),
                SwitchListTile(
                  title: const Text('Dark theme'),
                  subtitle: const Text('Terminal-style dark background'),
                  secondary: const Icon(Icons.dark_mode),
                  value: true,
                  onChanged: (v) {
                    // TODO: implement theme switching
                  },
                ),
                const Divider(),
                _sectionHeader(context, 'About'),
                const ListTile(
                  title: Text('rterm mobile'),
                  subtitle: Text('v0.1.0 -- SSH terminal client'),
                  leading: Icon(Icons.info_outline),
                ),
                ListTile(
                  title: const Text('Architecture'),
                  subtitle: const Text(
                    'Flutter WebView → rterm-wasm (egui/WASM) → WebTransport → rterm-relay',
                  ),
                  leading: const Icon(Icons.architecture),
                ),
              ],
            ),
    );
  }

  Widget _sectionHeader(BuildContext context, String title) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
      child: Text(
        title,
        style: Theme.of(context).textTheme.labelLarge?.copyWith(
              color: Theme.of(context).colorScheme.primary,
            ),
      ),
    );
  }

  void _editRelayUrl() {
    final ctl = TextEditingController(text: _relayUrlCtl.text);
    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Relay URL'),
        content: TextField(
          controller: ctl,
          keyboardType: TextInputType.url,
          autofocus: true,
          decoration: const InputDecoration(
            hintText: 'https://relay.example.com:4433',
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () {
              _relayUrlCtl.text = ctl.text;
              Navigator.pop(ctx);
              setState(() {});
              _saveRelayUrl();
            },
            child: const Text('OK'),
          ),
        ],
      ),
    );
  }
}
