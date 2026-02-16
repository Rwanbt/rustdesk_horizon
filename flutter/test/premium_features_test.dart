// Unit tests for 6 Premium UX Features.
// Verifies that all features are correctly implemented in source code.
//
// Run with: dart test/premium_features_test.dart
// (from the flutter/ directory)

import 'dart:io';

int _passed = 0;
int _failed = 0;

void expect(bool condition, String description) {
  if (condition) {
    _passed++;
    print('  PASS: $description');
  } else {
    _failed++;
    print('  FAIL: $description');
  }
}

void group(String name, void Function() body) {
  print('\n=== $name ===');
  body();
}

void test(String name, void Function() body) {
  print('  [$name]');
  body();
}

void main() {
  // Verify we're in the flutter/ directory
  if (!File('lib/consts.dart').existsSync()) {
    print('ERROR: Run from flutter/ directory.');
    exit(1);
  }

  final constsSource = File('lib/consts.dart').readAsStringSync();
  final modelSource = File('lib/models/model.dart').readAsStringSync();
  final commonSource = File('lib/common.dart').readAsStringSync();
  final remotePageSource =
      File('lib/mobile/pages/remote_page.dart').readAsStringSync();
  final settingsPageSource =
      File('lib/mobile/pages/settings_page.dart').readAsStringSync();
  final gestureHelpSource =
      File('lib/mobile/widgets/gesture_help.dart').readAsStringSync();
  final remoteInputSource =
      File('lib/common/widgets/remote_input.dart').readAsStringSync();
  final gestureMapModelSource =
      File('lib/models/gesture_map_model.dart').readAsStringSync();
  final themeSettingsPageSource =
      File('lib/mobile/pages/theme_settings_page.dart').readAsStringSync();

  // =====================================================================
  // Feature 4: Edge-Clamped Zoom
  // =====================================================================

  group('Feature 4: Edge-Clamped Zoom', () {
    test('minScale does NOT divide by 1.5', () {
      final minScaleIdx = modelSource.indexOf('double get minScale');
      expect(minScaleIdx != -1, 'minScale getter exists in model.dart');

      final bodyStart = modelSource.indexOf('{', minScaleIdx);
      final bodyEnd = modelSource.indexOf('}', bodyStart);
      final body = modelSource.substring(bodyStart, bodyEnd + 1);

      expect(!body.contains('/ 1.5'),
          'minScale does NOT divide by 1.5 (edge-clamped)');
      expect(body.contains('min(xscale, yscale)'),
          'minScale uses min(xscale, yscale) as base');
    });

    test('updateScale still clamps between min and max', () {
      final updateScaleIdx = modelSource.indexOf('void updateScale(');
      if (updateScaleIdx == -1) {
        final altIdx = modelSource.indexOf('updateScale(');
        expect(altIdx != -1, 'updateScale method exists (any signature)');
        if (altIdx != -1) {
          final bodyStart = modelSource.indexOf('{', altIdx);
          if (bodyStart != -1) {
            final end = (bodyStart + 500).clamp(0, modelSource.length);
            final afterBody = modelSource.substring(bodyStart, end);
            expect(
                afterBody.contains('minScale') || afterBody.contains('maxScale'),
                'updateScale references min/maxScale bounds');
          }
        }
      } else {
        final bodyStart = modelSource.indexOf('{', updateScaleIdx);
        final end = (bodyStart + 500).clamp(0, modelSource.length);
        final afterBody = modelSource.substring(bodyStart, end);
        expect(
            afterBody.contains('minScale') || afterBody.contains('maxScale'),
            'updateScale references min/maxScale bounds');
      }
    });
  });

  // =====================================================================
  // Feature 3: Hide Local Cursor
  // =====================================================================

  group('Feature 3: Hide Local Cursor', () {
    test('kOptionHideLocalCursor constant exists', () {
      expect(constsSource.contains('kOptionHideLocalCursor'),
          'kOptionHideLocalCursor defined in consts.dart');
      expect(constsSource.contains('"hide-local-cursor"'),
          'kOptionHideLocalCursor value is "hide-local-cursor"');
    });

    test('_hideLocalCursor getter exists in remote_page.dart', () {
      expect(remotePageSource.contains('_hideLocalCursor'),
          '_hideLocalCursor getter exists');
      expect(remotePageSource.contains('kOptionHideLocalCursor'),
          'Uses kOptionHideLocalCursor constant');
    });

    test('showCursorPaint checks _hideLocalCursor', () {
      final showCursorIdx = remotePageSource.indexOf('showCursorPaint');
      expect(showCursorIdx != -1, 'showCursorPaint exists');

      final bodyArea =
          remotePageSource.substring(showCursorIdx, showCursorIdx + 200);
      expect(bodyArea.contains('_hideLocalCursor'),
          'showCursorPaint checks _hideLocalCursor');
      expect(bodyArea.contains('isPeerAndroid'),
          'showCursorPaint still checks isPeerAndroid');
      expect(bodyArea.contains('cursorEmbedded'),
          'showCursorPaint still checks cursorEmbedded');
    });

    test('Hide local cursor checkbox in gesture_help.dart', () {
      expect(gestureHelpSource.contains('kOptionHideLocalCursor'),
          'gesture_help.dart references kOptionHideLocalCursor');
      expect(gestureHelpSource.contains('Hide local cursor'),
          'gesture_help.dart has "Hide local cursor" text');
    });
  });

  // =====================================================================
  // Feature 2: Theme & Glassmorphism
  // =====================================================================

  group('Feature 2: Theme & Glassmorphism', () {
    test('Theme option constants exist', () {
      expect(constsSource.contains('kOptionAccentColor'),
          'kOptionAccentColor defined');
      expect(constsSource.contains('"accent-color"'),
          'kOptionAccentColor value is "accent-color"');
      expect(constsSource.contains('kOptionSoberTheme'),
          'kOptionSoberTheme defined');
      expect(constsSource.contains('"sober-theme"'),
          'kOptionSoberTheme value is "sober-theme"');
    });

    test('dynamicAccent getter in MyTheme', () {
      expect(commonSource.contains('static Color get dynamicAccent'),
          'dynamicAccent getter exists in common.dart');
      expect(commonSource.contains('kOptionAccentColor'),
          'dynamicAccent reads kOptionAccentColor');
      expect(commonSource.contains('0xFF000000'),
          'dynamicAccent applies full opacity mask');
    });

    test('isSoberTheme getter in MyTheme', () {
      expect(commonSource.contains('static bool get isSoberTheme'),
          'isSoberTheme getter exists');
      expect(commonSource.contains('kOptionSoberTheme'),
          'isSoberTheme reads kOptionSoberTheme');
    });

    test('Bottom bar glassmorphism in remote_page.dart', () {
      expect(remotePageSource.contains('BackdropFilter'),
          'BackdropFilter used in remote_page.dart');
      expect(remotePageSource.contains('ImageFilter.blur'),
          'Blur filter applied');
      expect(remotePageSource.contains('isSoberTheme'),
          'Sober theme conditional applied');
    });

    test('FAB uses dynamicAccent', () {
      expect(remotePageSource.contains('FloatingActionButton'),
          'FloatingActionButton exists');
      expect(remotePageSource.contains('backgroundColor: MyTheme.dynamicAccent'),
          'FAB uses dynamicAccent for backgroundColor');
    });

    test('ThemeSettingsPage exists and has presets', () {
      expect(themeSettingsPageSource.contains('class ThemeSettingsPage'),
          'ThemeSettingsPage class exists');
      expect(themeSettingsPageSource.contains('presets'),
          'Color presets map exists');
      expect(themeSettingsPageSource.contains('0x0071FF'),
          'Default Blue preset defined');
      expect(themeSettingsPageSource.contains('0x009688'),
          'Teal preset defined');
      expect(themeSettingsPageSource.contains('0x7C4DFF'),
          'Purple preset defined');
      expect(themeSettingsPageSource.contains('Sober Theme'),
          'Sober Theme toggle exists');
    });

    test('Theme Customization tile in settings_page.dart', () {
      expect(settingsPageSource.contains('Theme Customization'),
          'Theme Customization tile exists in settings');
      expect(settingsPageSource.contains('ThemeSettingsPage'),
          'Navigation to ThemeSettingsPage exists');
      expect(settingsPageSource.contains('Icons.palette'),
          'Palette icon used for theme tile');
    });

    test('Theme Customization accessible from remote session', () {
      expect(remotePageSource.contains('ThemeSettingsPage'),
          'remote_page.dart references ThemeSettingsPage');
      expect(remotePageSource.contains('Theme Customization'),
          'Theme Customization menu entry in remote session');
      expect(remotePageSource.contains('Icons.palette'),
          'Palette icon used in remote session menu');
    });
  });

  // =====================================================================
  // Feature 1: Gesture Mapping System
  // =====================================================================

  group('Feature 1: Gesture Mapping System', () {
    test('gesture_map_model.dart enums and class', () {
      expect(gestureMapModelSource.contains('enum GestureInput'),
          'GestureInput enum exists');
      expect(gestureMapModelSource.contains('enum GestureAction'),
          'GestureAction enum exists');
      expect(gestureMapModelSource.contains('class GestureMapModel'),
          'GestureMapModel class exists');
    });

    test('GestureInput has all expected values', () {
      for (final input in [
        'tap1', 'tap2', 'doubleTap', 'longPress',
        'pan1', 'pan2', 'pan3', 'pinch', 'holdDrag'
      ]) {
        expect(gestureMapModelSource.contains('$input,') ||
               gestureMapModelSource.contains('$input\n'),
            'GestureInput.$input exists');
      }
    });

    test('GestureAction has all expected values', () {
      for (final action in [
        'leftClick', 'rightClick', 'doubleClick', 'scroll',
        'moveCursor', 'drag', 'panCanvas', 'zoomCanvas', 'nothing'
      ]) {
        expect(gestureMapModelSource.contains('$action,') ||
               gestureMapModelSource.contains('$action\n'),
            'GestureAction.$action exists');
      }
    });

    test('Default mouse mode mappings', () {
      expect(gestureMapModelSource.contains('defaultMouseMode'),
          'defaultMouseMode map exists');
      expect(
          gestureMapModelSource.contains(
              'GestureInput.tap1: GestureAction.leftClick'),
          'tap1 defaults to leftClick in mouse mode');
      expect(
          gestureMapModelSource.contains(
              'GestureInput.tap2: GestureAction.rightClick'),
          'tap2 defaults to rightClick in mouse mode');
      expect(
          gestureMapModelSource.contains(
              'GestureInput.pan3: GestureAction.scroll'),
          'pan3 defaults to scroll in mouse mode');
    });

    test('Default touch mode mappings', () {
      expect(gestureMapModelSource.contains('defaultTouchMode'),
          'defaultTouchMode map exists');
      expect(
          gestureMapModelSource
              .contains('GestureInput.pan1: GestureAction.drag'),
          'pan1 defaults to drag in touch mode');
    });

    test('getAction reads from local options with fallback', () {
      expect(gestureMapModelSource.contains('getAction'),
          'getAction method exists');
      expect(gestureMapModelSource.contains('mainGetLocalOption'),
          'getAction reads from local options');
      expect(gestureMapModelSource.contains('stored.isEmpty'),
          'getAction has fallback when stored is empty');
    });

    test('setAction writes to local options', () {
      expect(gestureMapModelSource.contains('setAction'),
          'setAction method exists');
      expect(gestureMapModelSource.contains('mainSetLocalOption'),
          'setAction writes to local options');
    });

    test('resetDefaults clears all options', () {
      expect(gestureMapModelSource.contains('resetDefaults'),
          'resetDefaults method exists');
      expect(
          gestureMapModelSource.contains("value: ''"),
          'resetDefaults clears values to empty string');
    });

    test('Option key format is gesture-{mode}-{input}', () {
      expect(gestureMapModelSource.contains("'gesture-\$mode-\${input.name}'"),
          'Option key format is gesture-{mode}-{input.name}');
    });

    test('Labels maps exist', () {
      expect(gestureMapModelSource.contains('gestureInputLabels'),
          'gestureInputLabels map exists');
      expect(gestureMapModelSource.contains('gestureActionLabels'),
          'gestureActionLabels map exists');
    });

    test('GestureMapModel helper methods for integrated panel', () {
      expect(gestureMapModelSource.contains('iconForInput'),
          'iconForInput method exists');
      expect(gestureMapModelSource.contains('cardInputLabels'),
          'cardInputLabels map exists');
      expect(gestureMapModelSource.contains('displayedTouchInputs'),
          'displayedTouchInputs list exists');
      expect(gestureMapModelSource.contains('displayedMouseInputs'),
          'displayedMouseInputs list exists');
      expect(gestureMapModelSource.contains('configurableInputs'),
          'configurableInputs set exists');
      expect(gestureMapModelSource.contains('isDefault'),
          'isDefault method exists');
    });

    test('Gesture settings merged into gesture_help.dart', () {
      // GestureSettingsPage was deleted — functionality is now inline
      expect(!File('lib/mobile/pages/gesture_settings_page.dart').existsSync(),
          'gesture_settings_page.dart was deleted (merged into gesture_help)');
      expect(gestureHelpSource.contains('_showActionPicker'),
          'Action picker dialog is inline in gesture_help.dart');
      expect(gestureHelpSource.contains('RadioListTile'),
          'RadioListTile used for action selection');
      expect(gestureHelpSource.contains('GestureMapModel.getAction'),
          'gesture_help reads actions from GestureMapModel');
      expect(gestureHelpSource.contains('GestureMapModel.setAction'),
          'gesture_help writes actions via GestureMapModel');
      expect(gestureHelpSource.contains('resetDefaults'),
          'Reset to defaults button exists');
      expect(gestureHelpSource.contains('_buildGestureCards'),
          'Dynamic gesture cards builder exists');
    });

    test('GestureInfo widget supports custom and tappable cards', () {
      expect(gestureHelpSource.contains('isCustom'),
          'GestureInfo has isCustom parameter');
      expect(gestureHelpSource.contains('VoidCallback? onTap'),
          'GestureInfo has onTap parameter');
      expect(gestureHelpSource.contains('InkWell'),
          'Tappable cards use InkWell');
      expect(gestureHelpSource.contains('FontWeight.bold'),
          'Custom mappings shown in bold');
      expect(gestureHelpSource.contains('Icons.edit'),
          'Edit icon shown on configurable cards');
    });

    test('remote_input.dart integrates gesture mapping', () {
      expect(remoteInputSource.contains("import 'package:flutter_hbb/models/gesture_map_model.dart'"),
          'remote_input.dart imports gesture_map_model');
      expect(remoteInputSource.contains('_dispatchTapAction'),
          '_dispatchTapAction method exists');
      expect(remoteInputSource.contains('GestureMapModel.getAction'),
          'remote_input.dart calls GestureMapModel.getAction');
    });

    test('Mouse mode callbacks use gesture mapping', () {
      expect(remoteInputSource.contains('GestureInput.tap1'),
          'onTap uses GestureInput.tap1');
      expect(remoteInputSource.contains('GestureInput.doubleTap'),
          'onDoubleTap uses GestureInput.doubleTap');
      expect(remoteInputSource.contains('GestureInput.longPress'),
          'onLongPress uses GestureInput.longPress');
      expect(remoteInputSource.contains('GestureInput.tap2'),
          'onDoubleFinerTap uses GestureInput.tap2');
    });

    test('Touch mode also uses gesture mapping', () {
      // Touch mode should now dispatch via GestureMapModel (getAction(true, ...))
      expect(remoteInputSource.contains('GestureMapModel.getAction(true, GestureInput.tap1)'),
          'Touch mode tap1 uses GestureMapModel');
      expect(remoteInputSource.contains('GestureMapModel.getAction(true, GestureInput.doubleTap)'),
          'Touch mode doubleTap uses GestureMapModel');
      expect(remoteInputSource.contains('GestureMapModel.getAction(true, GestureInput.longPress)'),
          'Touch mode longPress uses GestureMapModel');
    });

    test('_dispatchTapAction handles all tap actions', () {
      final dispatchIdx = remoteInputSource.indexOf('_dispatchTapAction');
      expect(dispatchIdx != -1, '_dispatchTapAction exists');
      final body = remoteInputSource.substring(dispatchIdx, dispatchIdx + 400);
      expect(body.contains('GestureAction.leftClick'),
          '_dispatchTapAction handles leftClick');
      expect(body.contains('GestureAction.rightClick'),
          '_dispatchTapAction handles rightClick');
      expect(body.contains('GestureAction.doubleClick'),
          '_dispatchTapAction handles doubleClick');
    });

    test('Three-finger pan uses gesture mapping', () {
      expect(remoteInputSource.contains('GestureInput.pan3'),
          'Three-finger pan uses GestureInput.pan3 mapping');
      expect(remoteInputSource.contains('GestureAction.scroll'),
          'Checks for scroll action');
      expect(remoteInputSource.contains('GestureAction.panCanvas'),
          'Checks for panCanvas action');
    });
  });

  // =====================================================================
  // Feature 5: Auto Keyboard (Client Side)
  // =====================================================================

  group('Feature 5: Auto Keyboard (Client Side)', () {
    test('kOptionAutoOpenKeyboard constant exists', () {
      expect(constsSource.contains('kOptionAutoOpenKeyboard'),
          'kOptionAutoOpenKeyboard defined');
      expect(constsSource.contains('"auto-open-keyboard"'),
          'kOptionAutoOpenKeyboard value is "auto-open-keyboard"');
    });

    test('openKeyboardCallback in FFI class', () {
      expect(modelSource.contains('openKeyboardCallback'),
          'openKeyboardCallback field exists in model.dart');
      expect(modelSource.contains('VoidCallback? openKeyboardCallback'),
          'openKeyboardCallback is VoidCallback?');
    });

    test('open_keyboard event handler in startEventListener', () {
      expect(modelSource.contains("name == 'open_keyboard'"),
          'open_keyboard event handler exists');
      expect(modelSource.contains('kOptionAutoOpenKeyboard'),
          'Event handler checks kOptionAutoOpenKeyboard option');
      expect(modelSource.contains('openKeyboardCallback'),
          'Event handler calls openKeyboardCallback');
    });

    test('Callback registration in remote_page.dart', () {
      expect(remotePageSource.contains('gFFI.openKeyboardCallback = openKeyboard'),
          'openKeyboardCallback registered in initState');
      expect(remotePageSource.contains('gFFI.openKeyboardCallback = null'),
          'openKeyboardCallback unregistered in dispose/close');
    });

    test('Auto open keyboard toggle in settings', () {
      expect(settingsPageSource.contains('kOptionAutoOpenKeyboard'),
          'Settings page references kOptionAutoOpenKeyboard');
      expect(settingsPageSource.contains('Auto open keyboard'),
          'Auto open keyboard toggle text exists');
    });
  });

  // =====================================================================
  // Feature 6: Auto-Rotation
  // =====================================================================

  group('Feature 6: Auto-Rotation', () {
    test('kOptionAutoRotation constant exists', () {
      expect(constsSource.contains('kOptionAutoRotation'),
          'kOptionAutoRotation defined');
      expect(constsSource.contains('"auto-rotation"'),
          'kOptionAutoRotation value is "auto-rotation"');
    });

    test('_sendOrientationResolution method exists', () {
      expect(remotePageSource.contains('_sendOrientationResolution'),
          '_sendOrientationResolution method exists');
      expect(remotePageSource.contains('sessionChangeResolution') ||
             remotePageSource.contains('Orientation orientation'),
          'Method takes Orientation parameter');
    });

    test('OrientationBuilder calls _sendOrientationResolution', () {
      final obIdx = remotePageSource.indexOf('OrientationBuilder');
      expect(obIdx != -1, 'OrientationBuilder exists');
      final end = (obIdx + 1200).clamp(0, remotePageSource.length);
      final obBody = remotePageSource.substring(obIdx, end);
      expect(obBody.contains('kOptionAutoRotation'),
          'OrientationBuilder checks kOptionAutoRotation');
      expect(obBody.contains('_sendOrientationResolution'),
          'OrientationBuilder calls _sendOrientationResolution');
    });

    test('Resolution swap logic in _sendOrientationResolution', () {
      final idx = remotePageSource.indexOf('void _sendOrientationResolution');
      expect(idx != -1, '_sendOrientationResolution method defined');
      if (idx != -1) {
        final end = (idx + 800).clamp(0, remotePageSource.length);
        final body = remotePageSource.substring(idx, end);
        expect(body.contains('Orientation.landscape'),
            'Handles landscape orientation');
        expect(
            body.contains('display.width') && body.contains('display.height'),
            'Uses display width and height');
        expect(body.contains('max(') && body.contains('min('),
            'Swaps dimensions using max/min');
      }
    });

    test('Auto rotation toggle in settings', () {
      expect(settingsPageSource.contains('kOptionAutoRotation'),
          'Settings page references kOptionAutoRotation');
      expect(settingsPageSource.contains('Auto rotation'),
          'Auto rotation toggle text exists');
    });
  });

  // =====================================================================
  // Cross-cutting: All option constants defined
  // =====================================================================

  group('Cross-cutting: Option constants', () {
    test('All 5 new option constants defined in consts.dart', () {
      final options = [
        'kOptionHideLocalCursor',
        'kOptionAccentColor',
        'kOptionSoberTheme',
        'kOptionAutoOpenKeyboard',
        'kOptionAutoRotation',
      ];
      for (final opt in options) {
        expect(constsSource.contains('const String $opt'),
            '$opt is const String in consts.dart');
      }
    });
  });

  // =====================================================================
  // Structural: Files exist
  // =====================================================================

  group('Structural: Files', () {
    test('gesture_map_model.dart exists', () {
      expect(File('lib/models/gesture_map_model.dart').existsSync(),
          'gesture_map_model.dart exists');
    });

    test('gesture_settings_page.dart removed (merged into gesture_help)', () {
      expect(!File('lib/mobile/pages/gesture_settings_page.dart').existsSync(),
          'gesture_settings_page.dart removed');
    });

    test('theme_settings_page.dart exists', () {
      expect(File('lib/mobile/pages/theme_settings_page.dart').existsSync(),
          'theme_settings_page.dart exists');
    });
  });

  // =====================================================================
  // UI Integrity: Gesture help centering and layout
  // =====================================================================

  group('UI Integrity: Gesture help layout', () {
    test('ToggleSwitch is centered in gesture_help.dart', () {
      final toggleIdx = gestureHelpSource.indexOf('ToggleSwitch(');
      expect(toggleIdx != -1, 'ToggleSwitch exists in gesture_help.dart');

      final beforeToggle = gestureHelpSource.substring(
          toggleIdx > 200 ? toggleIdx - 200 : 0, toggleIdx);
      final isCentered = beforeToggle.contains('Center') ||
          beforeToggle.contains('mainAxisAlignment: MainAxisAlignment.center') ||
          beforeToggle.contains('crossAxisAlignment: CrossAxisAlignment.center');
      expect(isCentered,
          'ToggleSwitch layout uses centering');
    });

    test('Gesture cards are dynamic (not hardcoded)', () {
      // The old hardcoded lists should be gone
      expect(!gestureHelpSource.contains("translate(\"Left Mouse\")"),
          'No hardcoded "Left Mouse" text');
      expect(!gestureHelpSource.contains("translate(\"Right Mouse\")"),
          'No hardcoded "Right Mouse" text');
      expect(gestureHelpSource.contains('_buildGestureCards'),
          'Dynamic _buildGestureCards method used');
      expect(gestureHelpSource.contains('gestureActionLabels'),
          'Uses gestureActionLabels from model');
    });
  });

  // =====================================================================
  // Summary
  // =====================================================================

  print('\n========================================');
  print('Results: $_passed passed, $_failed failed');
  print('========================================');

  if (_failed > 0) {
    exit(1);
  }
}
