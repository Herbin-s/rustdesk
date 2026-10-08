import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_hbb/desktop/pages/rdh_settings_page.dart';

String snapshot({
  bool enabled = true,
  bool? effective = true,
  bool ok = true,
  String reason = '',
}) =>
    jsonEncode({
      'ok': ok,
      'error': ok ? '' : 'Reload failed',
      'features': {
        for (final id in [
          'window-targeting',
          'memory-watchdog',
          'headless-terminal',
          'headless-file-transfer'
        ])
          id: {
            'enabled': id == 'window-targeting' ? enabled : true,
            'effective': id == 'window-targeting' ? effective : true,
            'reason': id == 'window-targeting' ? reason : '',
          }
      }
    });

Widget page({
  required Future<String> Function() read,
  required Future<String> Function(String, bool) write,
}) =>
    MaterialApp(
      home: Scaffold(
        body: RdhSettingsPage(
          readSettings: read,
          writeFeature: write,
          translateText: (text) => text,
        ),
      ),
    );

void main() {
  final windowSwitch = find.byKey(const ValueKey('window-targeting'));

  testWidgets('loading does not invent enabled switch values', (tester) async {
    final response = Completer<String>();
    await tester.pumpWidget(page(
      read: () => response.future,
      write: (_, __) async => snapshot(),
    ));
    expect(find.text('Loading RDH settings...'), findsOneWidget);
    expect(find.byType(Switch), findsNothing);

    response.complete(snapshot(effective: null, reason: 'service-unavailable'));
    await tester.pumpAndSettle();
    expect(tester.widget<Switch>(windowSwitch).value, isTrue);
    expect(find.text('Saved; current effect could not be confirmed'),
        findsOneWidget);
    expect(find.text('The background service could not be reached'),
        findsOneWidget);
  });

  testWidgets('save waits for authoritative readback before changing switch',
      (tester) async {
    final saved = Completer<String>();
    final readback = Completer<String>();
    var reads = 0;
    await tester.pumpWidget(page(
      read: () => reads++ == 0 ? Future.value(snapshot()) : readback.future,
      write: (feature, enabled) {
        expect(feature, 'window-targeting');
        expect(enabled, isFalse);
        return saved.future;
      },
    ));
    await tester.pumpAndSettle();
    await tester.tap(windowSwitch);
    await tester.pump();
    expect(tester.widget<Switch>(windowSwitch).value, isTrue);
    expect(tester.widget<Switch>(windowSwitch).onChanged, isNull);
    expect(find.text('Saving and checking...'), findsOneWidget);

    saved.complete(snapshot(enabled: false, effective: false));
    await tester.pump();
    expect(tester.widget<Switch>(windowSwitch).value, isTrue);
    readback.complete(snapshot(enabled: false, effective: false));
    await tester.pumpAndSettle();
    expect(tester.widget<Switch>(windowSwitch).value, isFalse);
    expect(tester.widget<Switch>(windowSwitch).onChanged, isNotNull);
    expect(find.text('Saved; still active'), findsNothing);
  });

  testWidgets('failed reload shows saved preference and still active warning',
      (tester) async {
    var current = snapshot();
    await tester.pumpWidget(page(
      read: () async => current,
      write: (_, __) async {
        current = snapshot(enabled: false, effective: true);
        return snapshot(enabled: false, effective: true, ok: false);
      },
    ));
    await tester.pumpAndSettle();
    await tester.tap(windowSwitch);
    await tester.pumpAndSettle();
    expect(tester.widget<Switch>(windowSwitch).value, isFalse);
    expect(find.text('Saved; still active'), findsOneWidget);
    expect(
      find.text(
          'Could not apply the change. The switches show the saved settings; check the status below each switch.'),
      findsOneWidget,
    );
  });
}
