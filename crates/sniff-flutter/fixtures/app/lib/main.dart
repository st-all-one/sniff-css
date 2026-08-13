import 'package:flutter/material.dart';

void main() {
  runApp(const SniffFixtureApp());
}

/// Deterministic fixture for sniff-flutter integration tests.
///
/// Geometry and colors are fixed so a capture can be asserted against them:
/// - Scaffold background `#2563eb`;
/// - white bold 24px `Text` on top of it → WCAG ratio ≈ 5.17 (AA pass);
/// - a disabled `ElevatedButton` (interactive element, still rendered).
class SniffFixtureApp extends StatelessWidget {
  const SniffFixtureApp({super.key});

  @override
  Widget build(BuildContext context) {
    return const MaterialApp(
      title: 'sniffCSS fixture',
      home: Scaffold(
        backgroundColor: Color(0xFF2563EB),
        body: Center(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                'Olá, sniffCSS',
                key: ValueKey('greeting'),
                style: TextStyle(
                  color: Colors.white,
                  fontSize: 24,
                  fontWeight: FontWeight.bold,
                ),
              ),
              SizedBox(height: 12),
              ElevatedButton(
                key: ValueKey('cta'),
                onPressed: null,
                child: Text('Button'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
