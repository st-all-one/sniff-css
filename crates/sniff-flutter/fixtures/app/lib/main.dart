import 'package:flutter/material.dart';
import 'package:flutter_driver/driver_extension.dart';

void main() {
  enableFlutterDriverExtension();
  runApp(const SniffFixtureApp());
}

/// Deterministic fixture for sniff-flutter integration tests.
///
/// Geometry and colors are fixed so a capture can be asserted against them:
/// - `ColoredBox` background `#2563eb` (exposed in diagnostics, unlike a
///   `Scaffold.backgroundColor`);
/// - white bold 24px `Text` on top of it → WCAG ratio ≈ 5.17 (AA pass);
/// - a disabled `ElevatedButton` (interactive element, still rendered);
/// - interactive elements for the `--action` validation: a `FilledButton`
///   counter, an `OutlinedButton` that opens a modal dialog, and a
///   `TextField` for `type` actions.
class SniffFixtureApp extends StatefulWidget {
  const SniffFixtureApp({super.key});

  @override
  State<SniffFixtureApp> createState() => _SniffFixtureAppState();
}

class _SniffFixtureAppState extends State<SniffFixtureApp> {
  int _count = 0;

  void _openModal(BuildContext context) {
    showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Modal'),
        content: const Text('hidden until tapped'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Close'),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'sniffCSS fixture',
      home: Scaffold(
        body: ColoredBox(
          color: const Color(0xFF2563EB),
          child: Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const Text(
                  'Olá, sniffCSS',
                  key: ValueKey('greeting'),
                  style: TextStyle(
                    color: Colors.white,
                    fontSize: 24,
                    fontWeight: FontWeight.bold,
                  ),
                ),
                const SizedBox(height: 12),
                const ElevatedButton(
                  key: ValueKey('cta'),
                  onPressed: null,
                  child: Text('Button'),
                ),
                const SizedBox(height: 12),
                FilledButton(
                  key: ValueKey('counter'),
                  onPressed: () => setState(() => _count++),
                  child: Text('Counter: $_count'),
                ),
                const SizedBox(height: 12),
                Builder(
                  builder: (context) => OutlinedButton(
                    key: ValueKey('modal'),
                    onPressed: () => _openModal(context),
                    child: const Text('Open modal'),
                  ),
                ),
                const SizedBox(height: 12),
                const TextField(
                  key: ValueKey('field'),
                  decoration: InputDecoration(hintText: 'Type here'),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
