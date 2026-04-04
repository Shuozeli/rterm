import 'package:flutter/material.dart';
import 'screens/host_list_screen.dart';

void main() {
  runApp(const RtermApp());
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
