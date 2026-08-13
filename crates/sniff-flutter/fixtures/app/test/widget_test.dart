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

    final scaffold = tester.widget<Scaffold>(find.byType(Scaffold));
    expect(scaffold.backgroundColor, const Color(0xFF2563EB));

    final text = tester.widget<Text>(find.text('Olá, sniffCSS'));
    expect(text.style?.color, Colors.white);
    expect(text.style?.fontSize, 24);
    expect(text.style?.fontWeight, FontWeight.bold);
  });
}
