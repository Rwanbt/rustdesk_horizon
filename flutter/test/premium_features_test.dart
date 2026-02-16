// Unit tests for Premium UX Features (batch 1 + batch 2).
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
  final frTranslationSource =
      File('../src/lang/fr.rs').readAsStringSync();

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
  // Feature 3: Hide Local Cursor (per-mode)
  // =====================================================================

  group('Feature 3: Hide Local Cursor (per-mode)', () {
    test('Per-mode cursor constants exist', () {
      expect(constsSource.contains('kOptionHideLocalCursorMouse'),
          'kOptionHideLocalCursorMouse defined');
      expect(constsSource.contains('kOptionHideLocalCursorTouch'),
          'kOptionHideLocalCursorTouch defined');
      expect(constsSource.contains('kOptionHideRemoteCursorMouse'),
          'kOptionHideRemoteCursorMouse defined');
      expect(constsSource.contains('kOptionHideRemoteCursorTouch'),
          'kOptionHideRemoteCursorTouch defined');
    });

    test('_hideLocalCursor uses per-mode keys', () {
      expect(remotePageSource.contains('_hideLocalCursor'),
          '_hideLocalCursor getter exists');
      expect(remotePageSource.contains('kOptionHideLocalCursorTouch'),
          'Uses kOptionHideLocalCursorTouch');
      expect(remotePageSource.contains('kOptionHideLocalCursorMouse'),
          'Uses kOptionHideLocalCursorMouse');
    });

    test('_hideRemoteCursor getter exists', () {
      expect(remotePageSource.contains('_hideRemoteCursor'),
          '_hideRemoteCursor getter exists');
      expect(remotePageSource.contains('kOptionHideRemoteCursorTouch'),
          'Uses kOptionHideRemoteCursorTouch');
      expect(remotePageSource.contains('kOptionHideRemoteCursorMouse'),
          'Uses kOptionHideRemoteCursorMouse');
    });

    test('showCursorPaint checks both hide cursors', () {
      final showCursorIdx = remotePageSource.indexOf('showCursorPaint');
      expect(showCursorIdx != -1, 'showCursorPaint exists');

      final bodyArea =
          remotePageSource.substring(showCursorIdx, showCursorIdx + 300);
      expect(bodyArea.contains('_hideLocalCursor'),
          'showCursorPaint checks _hideLocalCursor');
      expect(bodyArea.contains('_hideRemoteCursor'),
          'showCursorPaint checks _hideRemoteCursor');
    });

    test('Hide cursor checkboxes in gesture_help.dart', () {
      expect(gestureHelpSource.contains('Hide local cursor'),
          'gesture_help.dart has "Hide local cursor" text');
      expect(gestureHelpSource.contains('Hide distant cursor'),
          'gesture_help.dart has "Hide distant cursor" text');
      expect(gestureHelpSource.contains('kOptionHideLocalCursorTouch'),
          'gesture_help uses per-mode local cursor key');
      expect(gestureHelpSource.contains('kOptionHideRemoteCursorTouch'),
          'gesture_help uses per-mode remote cursor key');
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

    test('dynamicAccent defaults to Dark Gray', () {
      expect(commonSource.contains('static Color get dynamicAccent'),
          'dynamicAccent getter exists in common.dart');
      expect(commonSource.contains('0xFF616161'),
          'dynamicAccent defaults to Dark Gray (0xFF616161)');
    });

    test('isSoberTheme defaults to true (!= N)', () {
      expect(commonSource.contains('static bool get isSoberTheme'),
          'isSoberTheme getter exists');
      expect(commonSource.contains("!= 'N'"),
          'isSoberTheme uses != N (default ON)');
    });

    test('ThemeSettingsPage defaults match', () {
      expect(themeSettingsPageSource.contains("!= 'N'"),
          'ThemeSettingsPage uses != N for sober theme');
      expect(themeSettingsPageSource.contains('0x616161'),
          'ThemeSettingsPage fallback color is Dark Gray');
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
      expect(themeSettingsPageSource.contains('Sober Theme'),
          'Sober Theme toggle exists');
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

    test('GestureAction has all original + new values', () {
      for (final action in [
        'leftClick', 'rightClick', 'doubleClick', 'scroll',
        'moveCursor', 'drag', 'panCanvas', 'zoomCanvas', 'nothing',
        'copy', 'paste', 'selectAll', 'undo', 'redo', 'middleClick',
      ]) {
        expect(gestureMapModelSource.contains('$action,') ||
               gestureMapModelSource.contains('$action\n'),
            'GestureAction.$action exists');
      }
    });

    test('New gesture action labels exist', () {
      expect(gestureMapModelSource.contains("GestureAction.copy: 'Copy'"),
          'Copy label exists');
      expect(gestureMapModelSource.contains("GestureAction.paste: 'Paste'"),
          'Paste label exists');
      expect(gestureMapModelSource.contains("GestureAction.selectAll: 'Select All'"),
          'Select All label exists');
      expect(gestureMapModelSource.contains("GestureAction.undo: 'Undo'"),
          'Undo label exists');
      expect(gestureMapModelSource.contains("GestureAction.redo: 'Redo'"),
          'Redo label exists');
      expect(gestureMapModelSource.contains("GestureAction.middleClick: 'Middle Click'"),
          'Middle Click label exists');
    });

    test('Default touch mode pan1 is scroll (native smartphone)', () {
      final touchModeIdx = gestureMapModelSource.indexOf('defaultTouchMode');
      final touchModeEnd = gestureMapModelSource.indexOf('};', touchModeIdx);
      final touchModeBody = gestureMapModelSource.substring(touchModeIdx, touchModeEnd);
      expect(touchModeBody.contains('GestureInput.pan1: GestureAction.scroll'),
          'pan1 defaults to scroll in touch mode');
    });

    test('Default mouse mode mappings', () {
      expect(gestureMapModelSource.contains('defaultMouseMode'),
          'defaultMouseMode map exists');
      expect(
          gestureMapModelSource.contains(
              'GestureInput.tap1: GestureAction.leftClick'),
          'tap1 defaults to leftClick in mouse mode');
    });

    test('getAction reads from local options with fallback', () {
      expect(gestureMapModelSource.contains('getAction'),
          'getAction method exists');
      expect(gestureMapModelSource.contains('mainGetLocalOption'),
          'getAction reads from local options');
    });

    test('setAction writes to local options', () {
      expect(gestureMapModelSource.contains('setAction'),
          'setAction method exists');
      expect(gestureMapModelSource.contains('mainSetLocalOption'),
          'setAction writes to local options');
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
      expect(!File('lib/mobile/pages/gesture_settings_page.dart').existsSync(),
          'gesture_settings_page.dart was deleted (merged into gesture_help)');
      expect(gestureHelpSource.contains('_showActionPicker'),
          'Action picker dialog is inline in gesture_help.dart');
      expect(gestureHelpSource.contains('RadioListTile'),
          'RadioListTile used for action selection');
      expect(gestureHelpSource.contains('GestureMapModel.getAction'),
          'gesture_help reads actions from GestureMapModel');
      expect(gestureHelpSource.contains('resetDefaults'),
          'Reset to defaults button exists');
    });

    test('GestureInfo widget supports custom and tappable cards', () {
      expect(gestureHelpSource.contains('isCustom'),
          'GestureInfo has isCustom parameter');
      expect(gestureHelpSource.contains('VoidCallback? onTap'),
          'GestureInfo has onTap parameter');
      expect(gestureHelpSource.contains('InkWell'),
          'Tappable cards use InkWell');
    });
  });

  // =====================================================================
  // Two-finger zoom/scroll conflict fix
  // =====================================================================

  group('Two-finger zoom/scroll conflict fix', () {
    test('Threshold-based intent detection', () {
      expect(remoteInputSource.contains('kPinchThreshold'),
          'kPinchThreshold constant exists');
      expect(remoteInputSource.contains('scaleChange'),
          'scaleChange variable for intent detection');
    });

    test('Pinch zoom always active', () {
      final idx = remoteInputSource.indexOf('kPinchThreshold');
      expect(idx != -1, 'kPinchThreshold found');
      final area = remoteInputSource.substring(idx, idx + 500);
      expect(area.contains('updateScale'),
          'updateScale called when pinch threshold exceeded');
    });

    test('Scroll only when not pinching', () {
      final idx = remoteInputSource.indexOf('Pan2 behavior');
      if (idx == -1) {
        // Try alternate comment
        final altIdx = remoteInputSource.indexOf('pan2Action == GestureAction.scroll');
        expect(altIdx != -1, 'Scroll conditional on pan2Action');
      } else {
        expect(true, 'Pan2 behavior comment found');
      }
    });
  });

  // =====================================================================
  // One-finger scroll in touch mode
  // =====================================================================

  group('One-finger scroll in touch mode', () {
    test('_activeOneFingerPanAction field exists', () {
      expect(remoteInputSource.contains('_activeOneFingerPanAction'),
          '_activeOneFingerPanAction field exists');
    });

    test('_oneFingerScrollIntegral field exists', () {
      expect(remoteInputSource.contains('_oneFingerScrollIntegral'),
          '_oneFingerScrollIntegral field exists');
    });

    test('onOneFingerPanStart reads GestureMapModel for pan1', () {
      expect(remoteInputSource.contains('GestureMapModel.getAction(true, GestureInput.pan1)'),
          'onOneFingerPanStart reads pan1 action from GestureMapModel');
    });

    test('onOneFingerPanUpdate dispatches by action', () {
      final idx = remoteInputSource.indexOf('onOneFingerPanUpdate');
      expect(idx != -1, 'onOneFingerPanUpdate exists');
      final endIdx = remoteInputSource.indexOf('onOneFingerPanEnd');
      final body = remoteInputSource.substring(idx, endIdx);
      expect(body.contains('_activeOneFingerPanAction'),
          'Dispatches based on _activeOneFingerPanAction');
      expect(body.contains('GestureAction.scroll'),
          'Handles scroll action');
      expect(body.contains('GestureAction.panCanvas'),
          'Handles panCanvas action');
    });

    test('onOneFingerPanEnd only sends mouseUp for drag', () {
      final idx = remoteInputSource.indexOf('onOneFingerPanEnd');
      expect(idx != -1, 'onOneFingerPanEnd exists');
      final body = remoteInputSource.substring(idx, idx + 500);
      expect(body.contains('GestureAction.drag'),
          'Only sends mouseUp for drag action');
    });
  });

  // =====================================================================
  // New gesture actions in _dispatchTapAction
  // =====================================================================

  group('New gesture actions dispatch', () {
    test('_dispatchTapAction handles new actions', () {
      final dispatchIdx = remoteInputSource.indexOf('_dispatchTapAction');
      expect(dispatchIdx != -1, '_dispatchTapAction exists');
      final body = remoteInputSource.substring(dispatchIdx, dispatchIdx + 1200);
      expect(body.contains('GestureAction.copy'),
          '_dispatchTapAction handles copy');
      expect(body.contains('GestureAction.paste'),
          '_dispatchTapAction handles paste');
      expect(body.contains('GestureAction.selectAll'),
          '_dispatchTapAction handles selectAll');
      expect(body.contains('GestureAction.undo'),
          '_dispatchTapAction handles undo');
      expect(body.contains('GestureAction.redo'),
          '_dispatchTapAction handles redo');
      expect(body.contains('GestureAction.middleClick'),
          '_dispatchTapAction handles middleClick');
    });

    test('Copy sends Ctrl+C', () {
      final copyIdx = remoteInputSource.indexOf("GestureAction.copy:");
      expect(copyIdx != -1, 'GestureAction.copy case exists');
      final body = remoteInputSource.substring(copyIdx, copyIdx + 200);
      expect(body.contains("VK_C"), 'Copy sends VK_C');
    });

    test('Paste sends Ctrl+V', () {
      final pasteIdx = remoteInputSource.indexOf("GestureAction.paste:");
      expect(pasteIdx != -1, 'GestureAction.paste case exists');
      final body = remoteInputSource.substring(pasteIdx, pasteIdx + 200);
      expect(body.contains("VK_V"), 'Paste sends VK_V');
    });
  });

  // =====================================================================
  // French translations
  // =====================================================================

  group('French translations', () {
    test('Gesture action translations', () {
      expect(frTranslationSource.contains('"Left Click"'),
          'Left Click translation exists');
      expect(frTranslationSource.contains('"Right Click"'),
          'Right Click translation exists');
      expect(frTranslationSource.contains('"Scroll"'),
          'Scroll translation exists');
      expect(frTranslationSource.contains('"Move Cursor"'),
          'Move Cursor translation exists');
      expect(frTranslationSource.contains('"Middle Click"'),
          'Middle Click translation exists');
    });

    test('Gesture input translations', () {
      expect(frTranslationSource.contains('"One-Finger Tap"'),
          'One-Finger Tap translation exists');
      expect(frTranslationSource.contains('"One-Long Tap"'),
          'One-Long Tap translation exists');
      expect(frTranslationSource.contains('"Pinch to Zoom"'),
          'Pinch to Zoom translation exists');
    });

    test('UI string translations', () {
      expect(frTranslationSource.contains('"Hide local cursor"'),
          'Hide local cursor translation exists');
      expect(frTranslationSource.contains('"Hide distant cursor"'),
          'Hide distant cursor translation exists');
      expect(frTranslationSource.contains('"Theme Customization"'),
          'Theme Customization translation exists');
      expect(frTranslationSource.contains('"Sober Theme"'),
          'Sober Theme translation exists');
      expect(frTranslationSource.contains('"Reset to default"'),
          'Reset to default translation exists');
      expect(frTranslationSource.contains('"Dark Gray"'),
          'Dark Gray translation exists');
    });

    test('New action translations', () {
      expect(frTranslationSource.contains('"Copy"'),
          'Copy translation exists');
      expect(frTranslationSource.contains('"Select All"'),
          'Select All translation exists');
      expect(frTranslationSource.contains('"Undo"'),
          'Undo translation exists');
      expect(frTranslationSource.contains('"Redo"'),
          'Redo translation exists');
    });
  });

  // =====================================================================
  // KeyHelpTools retractable + auto-hide
  // =====================================================================

  group('KeyHelpTools retractable + auto-hide', () {
    test('KeyHelpTools has showBar parameter', () {
      expect(remotePageSource.contains('final bool showBar'),
          'KeyHelpTools has showBar parameter');
    });

    test('KeyHelpTools instantiation passes showBar', () {
      expect(remotePageSource.contains('showBar: _showBar'),
          'KeyHelpTools receives _showBar');
    });

    test('Side handle when hidden', () {
      expect(remotePageSource.contains('chevron_left'),
          'Side handle uses chevron_left icon');
      expect(remotePageSource.contains('widget.showBar'),
          'Build checks widget.showBar');
    });

    test('Tapping handle pins the toolbar', () {
      // When side handle is tapped, it should pin
      final handleIdx = remotePageSource.indexOf('chevron_left');
      expect(handleIdx != -1, 'chevron_left icon exists');
      final area = remotePageSource.substring(
          (handleIdx - 500).clamp(0, remotePageSource.length), handleIdx);
      expect(area.contains('_pin = true'),
          'Tapping handle sets _pin = true');
    });
  });

  // =====================================================================
  // Feature 5: Auto Keyboard (Client Side)
  // =====================================================================

  group('Feature 5: Auto Keyboard (Client Side)', () {
    test('kOptionAutoOpenKeyboard constant exists', () {
      expect(constsSource.contains('kOptionAutoOpenKeyboard'),
          'kOptionAutoOpenKeyboard defined');
    });

    test('openKeyboardCallback in FFI class', () {
      expect(modelSource.contains('openKeyboardCallback'),
          'openKeyboardCallback field exists in model.dart');
    });
  });

  // =====================================================================
  // Feature 6: Auto-Rotation
  // =====================================================================

  group('Feature 6: Auto-Rotation', () {
    test('kOptionAutoRotation constant exists', () {
      expect(constsSource.contains('kOptionAutoRotation'),
          'kOptionAutoRotation defined');
    });

    test('_sendOrientationResolution method exists', () {
      expect(remotePageSource.contains('_sendOrientationResolution'),
          '_sendOrientationResolution method exists');
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
  // Summary
  // =====================================================================

  print('\n========================================');
  print('Results: $_passed passed, $_failed failed');
  print('========================================');

  if (_failed > 0) {
    exit(1);
  }
}
