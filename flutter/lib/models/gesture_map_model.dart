import 'package:flutter/widgets.dart';
import 'package:flutter_hbb/models/platform_model.dart';

enum GestureInput {
  tap1,
  tap2,
  doubleTap,
  longPress,
  pan1,
  pan2,
  pan3,
  pinch,
  holdDrag,
}

enum GestureAction {
  leftClick,
  rightClick,
  doubleClick,
  scroll,
  moveCursor,
  drag,
  panCanvas,
  zoomCanvas,
  nothing,
  copy,
  paste,
  selectAll,
  undo,
  redo,
  middleClick,
  openKeyboard,
  textSelection,
}

const Map<GestureInput, String> gestureInputLabels = {
  GestureInput.tap1: 'One-Finger Tap',
  GestureInput.tap2: 'Two-Finger Tap',
  GestureInput.doubleTap: 'Double Tap',
  GestureInput.longPress: 'Long Press',
  GestureInput.pan1: 'One-Finger Pan',
  GestureInput.pan2: 'Two-Finger Pan',
  GestureInput.pan3: 'Three-Finger Pan',
  GestureInput.pinch: 'Pinch',
  GestureInput.holdDrag: 'Hold & Drag',
};

const Map<GestureAction, String> gestureActionLabels = {
  GestureAction.leftClick: 'Left Click',
  GestureAction.rightClick: 'Right Click',
  GestureAction.doubleClick: 'Double Click',
  GestureAction.scroll: 'Scroll',
  GestureAction.moveCursor: 'Move Cursor',
  GestureAction.drag: 'Mouse Drag',
  GestureAction.panCanvas: 'Pan Canvas',
  GestureAction.zoomCanvas: 'Zoom',
  GestureAction.nothing: 'Nothing',
  GestureAction.copy: 'Copy',
  GestureAction.paste: 'Paste',
  GestureAction.selectAll: 'Select All',
  GestureAction.undo: 'Undo',
  GestureAction.redo: 'Redo',
  GestureAction.middleClick: 'Middle Click',
  GestureAction.openKeyboard: 'Open Keyboard',
  GestureAction.textSelection: 'Text Selection',
};

class GestureMapModel {
  GestureMapModel._();

  static const Map<GestureInput, GestureAction> defaultMouseMode = {
    GestureInput.tap1: GestureAction.leftClick,
    GestureInput.tap2: GestureAction.rightClick,
    GestureInput.doubleTap: GestureAction.doubleClick,
    GestureInput.longPress: GestureAction.rightClick,
    GestureInput.pan1: GestureAction.moveCursor,
    GestureInput.pan2: GestureAction.scroll,
    GestureInput.pan3: GestureAction.panCanvas,
    GestureInput.pinch: GestureAction.zoomCanvas,
    GestureInput.holdDrag: GestureAction.drag,
  };

  static const Map<GestureInput, GestureAction> defaultTouchMode = {
    GestureInput.tap1: GestureAction.leftClick,
    GestureInput.tap2: GestureAction.nothing,
    GestureInput.doubleTap: GestureAction.doubleClick,
    GestureInput.longPress: GestureAction.rightClick,
    GestureInput.pan1: GestureAction.scroll,
    GestureInput.pan2: GestureAction.panCanvas,
    GestureInput.pan3: GestureAction.openKeyboard,
    GestureInput.pinch: GestureAction.zoomCanvas,
    GestureInput.holdDrag: GestureAction.textSelection,
  };

  static String _optionKey(bool touchMode, GestureInput input) {
    final mode = touchMode ? 'touch' : 'mouse';
    return 'gesture-$mode-${input.name}';
  }

  static GestureAction getAction(bool touchMode, GestureInput input) {
    final key = _optionKey(touchMode, input);
    final stored = bind.mainGetLocalOption(key: key);
    if (stored.isEmpty) {
      return (touchMode ? defaultTouchMode : defaultMouseMode)[input] ??
          GestureAction.nothing;
    }
    return GestureAction.values.firstWhere(
      (a) => a.name == stored,
      orElse: () => GestureAction.nothing,
    );
  }

  static Future<void> setAction(
      bool touchMode, GestureInput input, GestureAction action) async {
    final key = _optionKey(touchMode, input);
    await bind.mainSetLocalOption(key: key, value: action.name);
  }

  static Map<GestureInput, GestureAction> getDefaults(bool touchMode) {
    return touchMode ? defaultTouchMode : defaultMouseMode;
  }

  static Future<void> resetDefaults(bool touchMode) async {
    for (final input in GestureInput.values) {
      final key = _optionKey(touchMode, input);
      await bind.mainSetLocalOption(key: key, value: '');
    }
  }

  // --- Helpers for integrated gesture help panel ---

  static const String _iconFamily = 'gestureicons';

  static IconData iconForInput(GestureInput input) {
    switch (input) {
      case GestureInput.tap1:
        return const IconData(0xe9cd, fontFamily: _iconFamily);
      case GestureInput.longPress:
        return const IconData(0xe66b, fontFamily: _iconFamily);
      case GestureInput.pan1:
        return const IconData(0xe68f, fontFamily: _iconFamily);
      case GestureInput.holdDrag:
        return const IconData(0xe68f, fontFamily: _iconFamily);
      case GestureInput.pan3:
        return const IconData(0xe687, fontFamily: _iconFamily);
      case GestureInput.pan2:
        return const IconData(0xe686, fontFamily: _iconFamily);
      case GestureInput.pinch:
        return const IconData(0xe66a, fontFamily: _iconFamily);
      case GestureInput.tap2:
        return const IconData(0xe9cd, fontFamily: _iconFamily);
      case GestureInput.doubleTap:
        return const IconData(0xe691, fontFamily: _iconFamily);
    }
  }

  /// Friendly labels for the 6 displayed gesture cards.
  static const Map<GestureInput, String> cardInputLabels = {
    GestureInput.tap1: 'One-Finger Tap',
    GestureInput.longPress: 'One-Long Tap',
    GestureInput.pan1: 'One-Finger Move',
    GestureInput.holdDrag: 'Double Tap & Move',
    GestureInput.pan3: 'Three-Finger vertically',
    GestureInput.pan2: 'Two-Finger Move',
    GestureInput.pinch: 'Pinch to Zoom',
    GestureInput.tap2: 'Two-Finger Tap',
    GestureInput.doubleTap: 'Double Tap',
  };

  static const List<GestureInput> displayedTouchInputs = [
    GestureInput.tap1,
    GestureInput.longPress,
    GestureInput.pan1,
    GestureInput.holdDrag,
    GestureInput.pan3,
    GestureInput.pan2,
    GestureInput.pinch,
  ];

  static const List<GestureInput> displayedMouseInputs = [
    GestureInput.tap1,
    GestureInput.longPress,
    GestureInput.holdDrag,
    GestureInput.pan3,
    GestureInput.pan2,
    GestureInput.pinch,
  ];

  static const Set<GestureInput> configurableInputs = {
    GestureInput.tap1,
    GestureInput.tap2,
    GestureInput.doubleTap,
    GestureInput.longPress,
    GestureInput.pan1,
    GestureInput.holdDrag,
    GestureInput.pan2,
    GestureInput.pan3,
  };

  static bool isDefault(bool touchMode, GestureInput input) {
    final current = getAction(touchMode, input);
    final defaults = touchMode ? defaultTouchMode : defaultMouseMode;
    return current == (defaults[input] ?? GestureAction.nothing);
  }
}
