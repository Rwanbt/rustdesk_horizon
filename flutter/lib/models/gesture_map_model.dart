import 'package:flutter_hbb/consts.dart';
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
};

class GestureMapModel {
  GestureMapModel._();

  static const Map<GestureInput, GestureAction> defaultMouseMode = {
    GestureInput.tap1: GestureAction.leftClick,
    GestureInput.tap2: GestureAction.rightClick,
    GestureInput.doubleTap: GestureAction.doubleClick,
    GestureInput.longPress: GestureAction.rightClick,
    GestureInput.pan1: GestureAction.moveCursor,
    GestureInput.pan2: GestureAction.panCanvas,
    GestureInput.pan3: GestureAction.scroll,
    GestureInput.pinch: GestureAction.zoomCanvas,
    GestureInput.holdDrag: GestureAction.drag,
  };

  static const Map<GestureInput, GestureAction> defaultTouchMode = {
    GestureInput.tap1: GestureAction.leftClick,
    GestureInput.tap2: GestureAction.nothing,
    GestureInput.doubleTap: GestureAction.doubleClick,
    GestureInput.longPress: GestureAction.rightClick,
    GestureInput.pan1: GestureAction.drag,
    GestureInput.pan2: GestureAction.panCanvas,
    GestureInput.pan3: GestureAction.scroll,
    GestureInput.pinch: GestureAction.zoomCanvas,
    GestureInput.holdDrag: GestureAction.nothing,
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
}
