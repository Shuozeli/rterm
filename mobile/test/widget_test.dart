import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'package:rterm_mobile/main.dart';

void main() {
  setUp(() {
    // Provide empty shared_preferences for test environment.
    SharedPreferences.setMockInitialValues({});
  });

  testWidgets('App launches and shows host list', (WidgetTester tester) async {
    await tester.pumpWidget(const RtermApp());
    await tester.pumpAndSettle();

    // Verify the app bar shows "rterm"
    expect(find.text('rterm'), findsOneWidget);

    // Verify the empty state message is shown
    expect(find.text('No hosts configured'), findsOneWidget);
    expect(find.text('Tap + to add an SSH host'), findsOneWidget);

    // Verify the FAB is present
    expect(find.byIcon(Icons.add), findsOneWidget);

    // Verify settings button exists
    expect(find.byIcon(Icons.settings), findsOneWidget);
  });

  testWidgets('FAB navigates to host edit screen', (WidgetTester tester) async {
    await tester.pumpWidget(const RtermApp());
    await tester.pumpAndSettle();

    // Tap the add button
    await tester.tap(find.byIcon(Icons.add));
    await tester.pumpAndSettle();

    // Verify we are on the edit screen
    expect(find.text('New Host'), findsOneWidget);
    expect(find.text('Hostname'), findsOneWidget);
    expect(find.text('Port'), findsOneWidget);
    expect(find.text('Username'), findsOneWidget);
    expect(find.text('Save'), findsOneWidget);
  });

  testWidgets('Settings screen is accessible', (WidgetTester tester) async {
    await tester.pumpWidget(const RtermApp());
    await tester.pumpAndSettle();

    await tester.tap(find.byIcon(Icons.settings));
    await tester.pumpAndSettle();

    expect(find.text('Settings'), findsOneWidget);
    expect(find.text('Agent binary path'), findsOneWidget);
    expect(find.text('Font size'), findsOneWidget);
  });
}
