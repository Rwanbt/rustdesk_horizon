import 'package:flutter/material.dart';
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/models/gesture_map_model.dart';

class GestureSettingsPage extends StatefulWidget {
  const GestureSettingsPage({super.key});

  @override
  State<GestureSettingsPage> createState() => _GestureSettingsPageState();
}

class _GestureSettingsPageState extends State<GestureSettingsPage> {
  bool _touchMode = false;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        leading: IconButton(
            onPressed: () => Navigator.pop(context),
            icon: const Icon(Icons.arrow_back_ios)),
        title: Text(translate('Gesture Settings')),
        centerTitle: true,
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            tooltip: translate('Reset to default'),
            onPressed: () async {
              await GestureMapModel.resetDefaults(_touchMode);
              setState(() {});
            },
          ),
        ],
      ),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(12),
            child: SegmentedButton<bool>(
              segments: [
                ButtonSegment(
                    value: false,
                    label: Text(translate('Mouse Mode')),
                    icon: const Icon(Icons.mouse)),
                ButtonSegment(
                    value: true,
                    label: Text(translate('Touch Mode')),
                    icon: const Icon(Icons.touch_app)),
              ],
              selected: {_touchMode},
              onSelectionChanged: (sel) =>
                  setState(() => _touchMode = sel.first),
            ),
          ),
          const Divider(height: 1),
          Expanded(
            child: ListView(
              children: GestureInput.values.map((input) {
                final action =
                    GestureMapModel.getAction(_touchMode, input);
                final defaults =
                    GestureMapModel.getDefaults(_touchMode);
                final isDefault = action == defaults[input];
                return ListTile(
                  leading: Icon(_iconForInput(input)),
                  title: Text(translate(
                      gestureInputLabels[input] ?? input.name)),
                  subtitle: Text(
                    translate(gestureActionLabels[action] ?? action.name),
                    style: TextStyle(
                      color: isDefault ? null : MyTheme.dynamicAccent,
                      fontWeight:
                          isDefault ? FontWeight.normal : FontWeight.bold,
                    ),
                  ),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => _showActionPicker(input, action),
                );
              }).toList(),
            ),
          ),
        ],
      ),
    );
  }

  IconData _iconForInput(GestureInput input) {
    switch (input) {
      case GestureInput.tap1:
        return Icons.touch_app;
      case GestureInput.tap2:
        return Icons.back_hand;
      case GestureInput.doubleTap:
        return Icons.ads_click;
      case GestureInput.longPress:
        return Icons.pan_tool;
      case GestureInput.pan1:
        return Icons.swipe;
      case GestureInput.pan2:
        return Icons.swipe_right_alt;
      case GestureInput.pan3:
        return Icons.swipe_down_alt;
      case GestureInput.pinch:
        return Icons.pinch;
      case GestureInput.holdDrag:
        return Icons.open_with;
    }
  }

  void _showActionPicker(GestureInput input, GestureAction currentAction) {
    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(translate(gestureInputLabels[input] ?? input.name)),
        content: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: GestureAction.values.map((action) {
              return RadioListTile<GestureAction>(
                title: Text(translate(
                    gestureActionLabels[action] ?? action.name)),
                value: action,
                groupValue: currentAction,
                onChanged: (v) async {
                  if (v != null) {
                    await GestureMapModel.setAction(
                        _touchMode, input, v);
                    Navigator.pop(ctx);
                    setState(() {});
                  }
                },
              );
            }).toList(),
          ),
        ),
      ),
    );
  }
}
