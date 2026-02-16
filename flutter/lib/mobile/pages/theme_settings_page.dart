import 'package:flutter/material.dart';
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/consts.dart';
import 'package:flutter_hbb/models/platform_model.dart';

class ThemeSettingsPage extends StatefulWidget {
  const ThemeSettingsPage({super.key});

  @override
  State<ThemeSettingsPage> createState() => _ThemeSettingsPageState();
}

class _ThemeSettingsPageState extends State<ThemeSettingsPage> {
  static const Map<String, int> presets = {
    'Default Blue': 0x0071FF,
    'Teal': 0x009688,
    'Purple': 0x7C4DFF,
    'Orange': 0xFF6D00,
    'Red': 0xD50000,
    'Green': 0x2E7D32,
    'Dark Gray': 0x616161,
  };

  String _currentHex = '';
  bool _soberTheme = false;

  @override
  void initState() {
    super.initState();
    _currentHex = bind.mainGetLocalOption(key: kOptionAccentColor);
    _soberTheme = bind.mainGetLocalOption(key: kOptionSoberTheme) == 'Y';
  }

  int get _currentColor {
    if (_currentHex.isEmpty) return 0x0071FF;
    return int.tryParse(_currentHex, radix: 16) ?? 0x0071FF;
  }

  Future<void> _selectColor(int colorValue) async {
    final hex = colorValue.toRadixString(16).padLeft(6, '0').toUpperCase();
    await bind.mainSetLocalOption(key: kOptionAccentColor, value: hex);
    setState(() => _currentHex = hex);
  }

  Future<void> _clearColor() async {
    await bind.mainSetLocalOption(key: kOptionAccentColor, value: '');
    setState(() => _currentHex = '');
  }

  Future<void> _toggleSoberTheme(bool value) async {
    await bind.mainSetLocalOption(
        key: kOptionSoberTheme, value: value ? 'Y' : '');
    setState(() => _soberTheme = value);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        leading: IconButton(
            onPressed: () => Navigator.pop(context),
            icon: const Icon(Icons.arrow_back_ios)),
        title: Text(translate('Theme Customization')),
        centerTitle: true,
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(translate('Accent Color'),
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 12),
          Wrap(
            spacing: 12,
            runSpacing: 12,
            children: presets.entries.map((entry) {
              final color = Color(entry.value | 0xFF000000);
              final isSelected = _currentColor == entry.value;
              return GestureDetector(
                onTap: () => _selectColor(entry.value),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Container(
                      width: 48,
                      height: 48,
                      decoration: BoxDecoration(
                        color: color,
                        shape: BoxShape.circle,
                        border: isSelected
                            ? Border.all(color: Colors.white, width: 3)
                            : null,
                        boxShadow: isSelected
                            ? [
                                BoxShadow(
                                    color: color.withOpacity(0.6),
                                    blurRadius: 8,
                                    spreadRadius: 2)
                              ]
                            : null,
                      ),
                      child: isSelected
                          ? const Icon(Icons.check,
                              color: Colors.white, size: 24)
                          : null,
                    ),
                    const SizedBox(height: 4),
                    Text(translate(entry.key),
                        style: Theme.of(context).textTheme.bodySmall),
                  ],
                ),
              );
            }).toList(),
          ),
          if (_currentHex.isNotEmpty) ...[
            const SizedBox(height: 12),
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton.icon(
                onPressed: _clearColor,
                icon: const Icon(Icons.refresh, size: 18),
                label: Text(translate('Reset to default')),
              ),
            ),
          ],
          const SizedBox(height: 24),
          const Divider(),
          const SizedBox(height: 16),
          SwitchListTile(
            title: Text(translate('Sober Theme')),
            subtitle:
                Text(translate('Translucent toolbar with blur effect')),
            value: _soberTheme,
            onChanged: _toggleSoberTheme,
          ),
          const SizedBox(height: 16),
          Container(
            height: 56,
            decoration: BoxDecoration(
              color: MyTheme.dynamicAccent
                  .withOpacity(_soberTheme ? 0.5 : 1.0),
              borderRadius: BorderRadius.circular(8),
            ),
            alignment: Alignment.center,
            child: Text(translate('Preview'),
                style: const TextStyle(
                    color: Colors.white, fontWeight: FontWeight.bold)),
          ),
        ],
      ),
    );
  }
}
