// Smoke test for the sniffCSS fixture app: the fixed colors/text must be
// present, since the integration tests assert against them.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sniff_flutter_fixture/main.dart';

void main() {
  testWidgets('fixture renders the fixed colors and text', (tester) async {
    await tester.pumpWidget(const SniffFixtureApp());

    expect(find.text('Olá, sniffCSS'), findsOneWidget);
    expect(find.byType(ElevatedButton), findsOneWidget);
    expect(find.text('Button'), findsOneWidget);

    expect(find.byType(FilledButton), findsOneWidget);
    expect(find.text('Counter: 0'), findsOneWidget);
    expect(find.byType(OutlinedButton), findsOneWidget);
    expect(find.text('Open modal'), findsOneWidget);
    expect(find.byType(TextField), findsOneWidget);

    final box = tester
        .widgetList<ColoredBox>(find.byType(ColoredBox))
        .firstWhere((b) => b.color == const Color(0xFF2563EB));
    expect(box.color, const Color(0xFF2563EB));

    final text = tester.widget<Text>(find.text('Olá, sniffCSS'));
    expect(text.style?.color, Colors.white);
    expect(text.style?.fontSize, 24);
    expect(text.style?.fontWeight, FontWeight.bold);
  });

  testWidgets('interactive elements respond to taps', (tester) async {
    await tester.pumpWidget(const SniffFixtureApp());

    // Counter increments on tap.
    await tester.tap(find.byKey(const ValueKey('counter')));
    await tester.pumpAndSettle();
    expect(find.text('Counter: 1'), findsOneWidget);

    // The modal button reveals a dialog; Close dismisses it.
    await tester.tap(find.byKey(const ValueKey('modal')));
    await tester.pumpAndSettle();
    expect(find.text('hidden until tapped'), findsOneWidget);
    await tester.tap(find.text('Close'));
    await tester.pumpAndSettle();
    expect(find.text('hidden until tapped'), findsNothing);
  });
}
