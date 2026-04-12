// Quick test script to verify gRPC connectivity
// Run with: dart run test/grpc_client_test.dart

import 'package:rterm_mobile/services/grpc_client.dart';

Future<void> main() async {
  final client = RtermClient();

  print('Connecting to localhost:4434...');
  try {
    await client.connect('localhost', port: 4434);
    print('Connected!');

    print('Creating session...');
    final session = await client.createSession(
      name: 'test-session',
      shell: '/bin/bash',
      cols: 80,
      rows: 24,
    );
    print('Session created: $session');

    print('Getting snapshot...');
    final snapshot = await client.getSnapshot(session.name);
    print('Snapshot cols: ${snapshot.snapshot?.cols}');
    print('Snapshot rows: ${snapshot.snapshot?.numRows}');
    print('Snapshot title: ${snapshot.snapshot?.title}');
    print('Cursor row: ${snapshot.snapshot?.cursor?.row}');
    print('Cursor col: ${snapshot.snapshot?.cursor?.col}');

    print('Sending keys...');
    await client.sendKeys(session.name, [72, 101, 108, 108, 111]); // "Hello"
    print('Keys sent!');

    print('Disconnecting...');
    await client.disconnect();
    print('Done!');
  } catch (e) {
    print('Error: $e');
  }
}
