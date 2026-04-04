import 'package:flutter/material.dart';
import '../models/host_profile.dart';
import '../services/host_storage.dart';
import 'host_edit_screen.dart';
import 'terminal_screen.dart';
import 'settings_screen.dart';

class HostListScreen extends StatefulWidget {
  const HostListScreen({super.key});

  @override
  State<HostListScreen> createState() => _HostListScreenState();
}

class _HostListScreenState extends State<HostListScreen> {
  final _storage = HostStorage();
  List<HostProfile> _hosts = [];
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _loadHosts();
  }

  Future<void> _loadHosts() async {
    final hosts = await _storage.loadAll();
    setState(() {
      _hosts = hosts;
      _loading = false;
    });
  }

  Future<void> _deleteHost(String id) async {
    await _storage.delete(id);
    await _loadHosts();
  }

  void _navigateToEdit({HostProfile? profile}) async {
    final result = await Navigator.push<HostProfile>(
      context,
      MaterialPageRoute(
        builder: (_) => HostEditScreen(profile: profile),
      ),
    );
    if (result != null) {
      await _storage.save(result);
      await _loadHosts();
    }
  }

  void _connectToHost(HostProfile host) {
    Navigator.push(
      context,
      MaterialPageRoute(
        builder: (_) => TerminalScreen(host: host),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('rterm'),
        actions: [
          IconButton(
            icon: const Icon(Icons.settings),
            onPressed: () => Navigator.push(
              context,
              MaterialPageRoute(builder: (_) => const SettingsScreen()),
            ),
          ),
        ],
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : _hosts.isEmpty
              ? Center(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        Icons.terminal,
                        size: 64,
                        color: Theme.of(context).colorScheme.outline,
                      ),
                      const SizedBox(height: 16),
                      Text(
                        'No hosts configured',
                        style: Theme.of(context).textTheme.titleMedium,
                      ),
                      const SizedBox(height: 8),
                      Text(
                        'Tap + to add an SSH host',
                        style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                              color: Theme.of(context).colorScheme.outline,
                            ),
                      ),
                    ],
                  ),
                )
              : ListView.builder(
                  itemCount: _hosts.length,
                  itemBuilder: (context, index) {
                    final host = _hosts[index];
                    return Dismissible(
                      key: Key(host.id),
                      direction: DismissDirection.endToStart,
                      background: Container(
                        color: Theme.of(context).colorScheme.error,
                        alignment: Alignment.centerRight,
                        padding: const EdgeInsets.only(right: 16),
                        child: Icon(
                          Icons.delete,
                          color: Theme.of(context).colorScheme.onError,
                        ),
                      ),
                      confirmDismiss: (_) async {
                        return await showDialog<bool>(
                              context: context,
                              builder: (ctx) => AlertDialog(
                                title: const Text('Delete host?'),
                                content: Text(
                                  'Remove "${host.name}" from saved hosts?',
                                ),
                                actions: [
                                  TextButton(
                                    onPressed: () =>
                                        Navigator.pop(ctx, false),
                                    child: const Text('Cancel'),
                                  ),
                                  TextButton(
                                    onPressed: () =>
                                        Navigator.pop(ctx, true),
                                    child: const Text('Delete'),
                                  ),
                                ],
                              ),
                            ) ??
                            false;
                      },
                      onDismissed: (_) => _deleteHost(host.id),
                      child: ListTile(
                        leading: CircleAvatar(
                          child: Text(
                            host.name.isNotEmpty
                                ? host.name[0].toUpperCase()
                                : '?',
                          ),
                        ),
                        title: Text(host.name),
                        subtitle: Text(
                          '${host.username}@${host.hostname}:${host.port}',
                        ),
                        trailing: Icon(
                          host.authType == 'key'
                              ? Icons.vpn_key
                              : Icons.password,
                          size: 20,
                          color: Theme.of(context).colorScheme.outline,
                        ),
                        onTap: () => _connectToHost(host),
                        onLongPress: () => _navigateToEdit(profile: host),
                      ),
                    );
                  },
                ),
      floatingActionButton: FloatingActionButton(
        onPressed: () => _navigateToEdit(),
        child: const Icon(Icons.add),
      ),
    );
  }
}
