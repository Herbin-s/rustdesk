import 'dart:convert';

import 'package:flutter/material.dart';

const _features = [
  (
    id: 'window-targeting',
    title: 'Remote window activation',
    description:
        'Activate the window you click during remote control. Turn off to use official RustDesk click handling.',
  ),
  (
    id: 'memory-watchdog',
    title: 'Scheduled memory recovery',
    description:
        'Restart the background service at 06:00 if its memory exceeds the configured limit. Turn off to prevent this scheduled restart.',
  ),
  (
    id: 'headless-terminal',
    title: 'Terminal without a window',
    description:
        'Allow new local terminal commands to connect without opening a window. Existing sessions and remote terminal permissions are unaffected.',
  ),
  (
    id: 'headless-file-transfer',
    title: 'File transfer without a window',
    description:
        'Allow new local file transfer commands to run without opening a window. Existing transfers and remote file permissions are unaffected.',
  ),
];

const _reasonText = {
  'service-unavailable': 'The background service could not be reached',
  'invalid-preferences': 'The saved settings could not be read',
  'threshold-disabled':
      'Memory recovery is disabled by the configured memory limit',
  'not-supervised':
      'The background service is not running in the required mode',
  'watchdog-unavailable': 'Scheduled memory recovery is unavailable',
};

class RdhSettingsPage extends StatefulWidget {
  final Future<String> Function() readSettings;
  final Future<String> Function(String feature, bool enabled) writeFeature;
  final String Function(String) translateText;

  const RdhSettingsPage({
    super.key,
    required this.readSettings,
    required this.writeFeature,
    required this.translateText,
  });

  @override
  State<RdhSettingsPage> createState() => _RdhSettingsPageState();
}

class _RdhSettingsPageState extends State<RdhSettingsPage> {
  Map<String, ({bool enabled, bool? effective, String reason})>? _settings;
  bool _loading = true;
  bool _stale = false;
  String? _saving;
  String? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final response = _decode(await widget.readSettings());
      if (!response.ok) throw const FormatException('Settings unavailable');
      if (!mounted) return;
      setState(() {
        _settings = response.features;
        _stale = false;
      });
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _stale = true;
        _error = 'Could not read current settings. Refresh to try again.';
      });
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _save(String feature, bool enabled) async {
    setState(() {
      _saving = feature;
      _error = null;
    });
    var saved = false;
    try {
      saved = _decode(await widget.writeFeature(feature, enabled)).ok;
    } catch (_) {
      // A failed response can still follow a persisted change; always read back.
    }
    try {
      final response = _decode(await widget.readSettings());
      if (!response.ok) throw const FormatException('Settings unavailable');
      if (!mounted) return;
      setState(() {
        _settings = response.features;
        _stale = false;
        if (!saved || response.features[feature]!.enabled != enabled) {
          _error =
              'Could not apply the change. The switches show the saved settings; check the status below each switch.';
        }
      });
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _stale = true;
        _error =
            'Could not confirm the change. Refresh before changing another setting.';
      });
    } finally {
      if (mounted) setState(() => _saving = null);
    }
  }

  @override
  Widget build(BuildContext context) {
    final text = widget.translateText;
    if (_settings == null && _loading) {
      return Center(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          const CircularProgressIndicator(),
          const SizedBox(height: 12),
          Text(text('Loading RDH settings...')),
        ]),
      );
    }
    return ListView(
      padding: const EdgeInsets.only(bottom: 15),
      children: [
        _card(Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Row(children: [
            Expanded(
              child: Text(text('RDH enhancements'),
                  style: const TextStyle(fontSize: 20)),
            ),
            IconButton(
              tooltip: text('Refresh settings'),
              onPressed: _loading || _saving != null ? null : _load,
              icon: const Icon(Icons.refresh),
            ),
          ]),
          Text(text(
              'Each switch shows the saved setting. Its status confirms whether it is currently in effect.')),
          if (_loading) ...[
            const SizedBox(height: 12),
            const LinearProgressIndicator(),
          ],
          if (_error != null) ...[
            const SizedBox(height: 12),
            Text(text(_error!),
                style: TextStyle(color: Theme.of(context).colorScheme.error)),
          ],
        ])),
        if (_settings != null)
          for (final feature in _features)
            _featureCard(
                context, feature.id, feature.title, feature.description),
      ],
    );
  }

  Widget _featureCard(
      BuildContext context, String id, String title, String description) {
    final text = widget.translateText;
    final feature = _settings![id]!;
    final saving = _saving == id;
    final status = _stale
        ? 'Current status is unconfirmed'
        : saving
            ? 'Saving and checking...'
            : feature.effective == null
                ? 'Saved; current effect could not be confirmed'
                : feature.effective != feature.enabled
                    ? feature.effective!
                        ? 'Saved; still active'
                        : 'Saved; currently inactive'
                    : id.startsWith('headless-')
                        ? feature.enabled
                            ? 'Applies to new commands'
                            : 'Disabled for new commands'
                        : 'Setting applied';
    final warning = _stale ||
        feature.effective == null ||
        feature.effective != feature.enabled;
    final reason = _reasonText[feature.reason];
    return _card(
        Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Row(children: [
        Expanded(
          child: Text(text(title), style: const TextStyle(fontSize: 20)),
        ),
        Switch(
          key: ValueKey(id),
          value: feature.enabled,
          onChanged: _loading || _saving != null || _stale
              ? null
              : (enabled) => _save(id, enabled),
        ),
      ]),
      if (warning && !saving && !_stale && reason != null) ...[
        const SizedBox(height: 6),
        Text(text(reason), style: const TextStyle(fontSize: 13)),
      ],
      Text(text(description)),
      const SizedBox(height: 10),
      Row(children: [
        if (saving)
          const SizedBox(
              width: 14,
              height: 14,
              child: CircularProgressIndicator(strokeWidth: 2))
        else
          Icon(warning ? Icons.info_outline : Icons.check_circle_outline,
              size: 16,
              color: warning ? Theme.of(context).colorScheme.error : null),
        const SizedBox(width: 6),
        Expanded(
          child: Text(text(status), style: const TextStyle(fontSize: 13)),
        ),
      ]),
    ]));
  }

  Widget _card(Widget child) => Row(children: [
        Flexible(
          child: SizedBox(
            width: 540,
            child: Padding(
              padding: const EdgeInsets.only(left: 15, top: 15),
              child: Card(
                child: Padding(padding: const EdgeInsets.all(15), child: child),
              ),
            ),
          ),
        ),
      ]);
}

({
  bool ok,
  Map<String, ({bool enabled, bool? effective, String reason})> features
}) _decode(String response) {
  final decoded = jsonDecode(response);
  if (decoded is! Map<String, dynamic> || decoded['ok'] is! bool) {
    throw const FormatException('Invalid settings response');
  }
  final rawFeatures = decoded['features'];
  if (rawFeatures is! Map<String, dynamic>) {
    throw const FormatException('Missing settings');
  }
  final features = <String, ({bool enabled, bool? effective, String reason})>{};
  for (final feature in _features) {
    final raw = rawFeatures[feature.id];
    if (raw is! Map<String, dynamic> ||
        raw['enabled'] is! bool ||
        !raw.containsKey('effective') ||
        (raw['effective'] != null && raw['effective'] is! bool) ||
        (raw['reason'] != null && raw['reason'] is! String)) {
      throw const FormatException('Invalid feature setting');
    }
    features[feature.id] = (
      enabled: raw['enabled'] as bool,
      effective: raw['effective'] as bool?,
      reason: raw['reason'] as String? ?? '',
    );
  }
  return (ok: decoded['ok'] as bool, features: features);
}
