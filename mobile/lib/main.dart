import 'package:flutter/material.dart';
import 'screens/host_list_screen.dart';
import 'services/host_storage.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // Initialize default settings
  await _initDefaults();

  runApp(const RtermApp());
}

Future<void> _initDefaults() async {
  final storage = HostStorage();

  // Initialize default hosts for testing
  await storage.initializeDefaultHosts();

  // Set default relay URL if not configured
  final settings = await storage.loadSettings();
  if (!settings.containsKey('relay_url') || settings['relay_url']!.isEmpty) {
    settings['relay_url'] = '100.95.116.72';
    await storage.saveSettings(settings);
  }
}

class RtermApp extends StatelessWidget {
  const RtermApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'rterm',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        colorSchemeSeed: Colors.teal,
        brightness: Brightness.dark,
        useMaterial3: true,
      ),
      home: const HostListScreen(),
    );
  }
}
