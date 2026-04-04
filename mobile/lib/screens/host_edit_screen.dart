import 'package:flutter/material.dart';
import '../models/host_profile.dart';

class HostEditScreen extends StatefulWidget {
  final HostProfile? profile;

  const HostEditScreen({super.key, this.profile});

  @override
  State<HostEditScreen> createState() => _HostEditScreenState();
}

class _HostEditScreenState extends State<HostEditScreen> {
  final _formKey = GlobalKey<FormState>();
  late final TextEditingController _nameCtl;
  late final TextEditingController _hostnameCtl;
  late final TextEditingController _portCtl;
  late final TextEditingController _usernameCtl;
  late final TextEditingController _passwordCtl;
  late final TextEditingController _keyCtl;
  late final TextEditingController _relayUrlCtl;
  late String _authType;

  bool get _isEditing => widget.profile != null;

  @override
  void initState() {
    super.initState();
    final p = widget.profile;
    _nameCtl = TextEditingController(text: p?.name ?? '');
    _hostnameCtl = TextEditingController(text: p?.hostname ?? '');
    _portCtl = TextEditingController(text: (p?.port ?? 22).toString());
    _usernameCtl = TextEditingController(text: p?.username ?? 'root');
    _passwordCtl = TextEditingController(text: p?.password ?? '');
    _keyCtl = TextEditingController(text: p?.privateKey ?? '');
    _relayUrlCtl = TextEditingController(text: p?.relayUrl ?? '');
    _authType = p?.authType ?? 'password';
  }

  @override
  void dispose() {
    _nameCtl.dispose();
    _hostnameCtl.dispose();
    _portCtl.dispose();
    _usernameCtl.dispose();
    _passwordCtl.dispose();
    _keyCtl.dispose();
    _relayUrlCtl.dispose();
    super.dispose();
  }

  void _save() {
    if (!_formKey.currentState!.validate()) return;
    final profile = HostProfile(
      id: widget.profile?.id,
      name: _nameCtl.text.trim(),
      hostname: _hostnameCtl.text.trim(),
      port: int.tryParse(_portCtl.text) ?? 22,
      username: _usernameCtl.text.trim(),
      authType: _authType,
      password: _authType == 'password' ? _passwordCtl.text : null,
      privateKey: _authType == 'key' ? _keyCtl.text : null,
      relayUrl: _relayUrlCtl.text.trim().isEmpty ? null : _relayUrlCtl.text.trim(),
    );
    Navigator.pop(context, profile);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(_isEditing ? 'Edit Host' : 'New Host'),
        actions: [
          TextButton(
            onPressed: _save,
            child: const Text('Save'),
          ),
        ],
      ),
      body: Form(
        key: _formKey,
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            TextFormField(
              controller: _nameCtl,
              decoration: const InputDecoration(
                labelText: 'Name',
                hintText: 'e.g. prod-server',
              ),
              validator: (v) =>
                  (v == null || v.trim().isEmpty) ? 'Required' : null,
            ),
            const SizedBox(height: 16),
            TextFormField(
              controller: _hostnameCtl,
              decoration: const InputDecoration(
                labelText: 'Hostname',
                hintText: 'e.g. 10.0.0.5',
              ),
              validator: (v) =>
                  (v == null || v.trim().isEmpty) ? 'Required' : null,
            ),
            const SizedBox(height: 16),
            TextFormField(
              controller: _portCtl,
              decoration: const InputDecoration(
                labelText: 'Port',
              ),
              keyboardType: TextInputType.number,
              validator: (v) {
                final port = int.tryParse(v ?? '');
                if (port == null || port < 1 || port > 65535) {
                  return 'Enter a valid port (1-65535)';
                }
                return null;
              },
            ),
            const SizedBox(height: 16),
            TextFormField(
              controller: _usernameCtl,
              decoration: const InputDecoration(
                labelText: 'Username',
              ),
              validator: (v) =>
                  (v == null || v.trim().isEmpty) ? 'Required' : null,
            ),
            const SizedBox(height: 16),
            SegmentedButton<String>(
              segments: const [
                ButtonSegment(
                  value: 'password',
                  label: Text('Password'),
                  icon: Icon(Icons.password),
                ),
                ButtonSegment(
                  value: 'key',
                  label: Text('SSH Key'),
                  icon: Icon(Icons.vpn_key),
                ),
              ],
              selected: {_authType},
              onSelectionChanged: (sel) =>
                  setState(() => _authType = sel.first),
            ),
            const SizedBox(height: 16),
            if (_authType == 'password')
              TextFormField(
                controller: _passwordCtl,
                decoration: const InputDecoration(
                  labelText: 'Password',
                ),
                obscureText: true,
              )
            else
              TextFormField(
                controller: _keyCtl,
                decoration: const InputDecoration(
                  labelText: 'Private Key (PEM)',
                  hintText: '-----BEGIN OPENSSH PRIVATE KEY-----',
                ),
                maxLines: 5,
              ),
            const SizedBox(height: 16),
            TextFormField(
              controller: _relayUrlCtl,
              decoration: const InputDecoration(
                labelText: 'Relay URL (optional)',
                hintText: 'e.g. https://relay.example.com:4433',
              ),
              keyboardType: TextInputType.url,
            ),
          ],
        ),
      ),
    );
  }
}
