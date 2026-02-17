import 'dart:convert';
import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/gestures.dart';

import 'package:flutter_hbb/models/platform_model.dart';
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/consts.dart';
import 'package:flutter_hbb/models/model.dart';
import 'package:flutter_hbb/models/input_model.dart';

import 'package:flutter_hbb/models/gesture_map_model.dart';

import './gestures.dart';

class RawKeyFocusScope extends StatelessWidget {
  final FocusNode? focusNode;
  final ValueChanged<bool>? onFocusChange;
  final InputModel inputModel;
  final Widget child;

  RawKeyFocusScope({
    this.focusNode,
    this.onFocusChange,
    required this.inputModel,
    required this.child,
  });

  @override
  Widget build(BuildContext context) {
    // https://github.com/flutter/flutter/issues/154053
    final useRawKeyEvents = isLinux && !isWeb;
    // FIXME: On Windows, `AltGr` will generate `Alt` and `Control` key events,
    // while `Alt` and `Control` are seperated key events for en-US input method.
    return FocusScope(
        autofocus: true,
        child: Focus(
            autofocus: true,
            canRequestFocus: true,
            focusNode: focusNode,
            onFocusChange: onFocusChange,
            onKey: useRawKeyEvents
                ? (FocusNode data, RawKeyEvent event) =>
                    inputModel.handleRawKeyEvent(event)
                : null,
            onKeyEvent: useRawKeyEvents
                ? null
                : (FocusNode node, KeyEvent event) =>
                    inputModel.handleKeyEvent(event),
            child: child));
  }
}

// For virtual mouse when using the mouse mode on mobile.
// Special hold-drag mode: one finger holds a button (left/right button), another finger pans.
// This flag is to override the scale gesture to a pan gesture.
bool isSpecialHoldDragActive = false;
// Cache the last focal point to calculate deltas in special hold-drag mode.
Offset _lastSpecialHoldDragFocalPoint = Offset.zero;

class RawTouchGestureDetectorRegion extends StatefulWidget {
  final Widget child;
  final FFI ffi;
  final bool isCamera;
  final VoidCallback? onInteraction;
  final VoidCallback? onOpenKeyboard;
  late final InputModel inputModel = ffi.inputModel;
  late final FfiModel ffiModel = ffi.ffiModel;

  RawTouchGestureDetectorRegion({
    required this.child,
    required this.ffi,
    this.isCamera = false,
    this.onInteraction,
    this.onOpenKeyboard,
  });

  @override
  State<RawTouchGestureDetectorRegion> createState() =>
      _RawTouchGestureDetectorRegionState();
}

/// touchMode only:
///   LongPress -> right click
///   OneFingerPan -> start/end -> left down start/end
///   onDoubleTapDown -> move to
///   onLongPressDown => move to
///
/// mouseMode only:
///   DoubleFiner -> right click
///   HoldDrag -> left drag
class _RawTouchGestureDetectorRegionState
    extends State<RawTouchGestureDetectorRegion> {
  Offset _cacheLongPressPosition = Offset(0, 0);
  // Timestamp of the last long press event.
  int _cacheLongPressPositionTs = 0;
  double _mouseScrollIntegral = 0; // mouse scroll speed controller
  double _scale = 1;
  GestureAction? _activeOneFingerPanAction;
  double _oneFingerScrollIntegral = 0;
  GestureAction? _activeHoldDragAction;
  double _holdDragScrollIntegral = 0;
  bool _threeFingerOneShotFired = false;
  // Two-finger intent locking with cumulative detection
  // null = undetermined, true = pinch/zoom, false = pan2 action
  bool? _twoFingerIsPinch;
  bool _twoFingerOneShotFired = false;
  double _twoFingerCumulativeScale = 0; // accumulated |scale - 1.0|
  double _twoFingerCumulativeTranslation = 0; // accumulated focal point distance

  // Workaround tap down event when two fingers are used to scale(mobile)
  TapDownDetails? _lastTapDownDetails;

  PointerDeviceKind? lastDeviceKind;

  // For touch mode, onDoubleTap
  // `onDoubleTap()` does not provide the position of the tap event.
  Offset _lastPosOfDoubleTapDown = Offset.zero;
  bool _touchModePanStarted = false;
  Offset _doubleFinerTapPosition = Offset.zero;

  // For mouse mode, we need to block the events when the cursor is in a blocked area.
  // So we need to cache the last tap down position.
  Offset? _lastTapDownPositionForMouseMode;
  // Cache global position for onTap (which lacks position info).
  Offset? _lastTapDownGlobalPosition;

  FFI get ffi => widget.ffi;
  FfiModel get ffiModel => widget.ffiModel;
  InputModel get inputModel => widget.inputModel;
  bool get handleTouch => (isDesktop || isWebDesktop) || ffiModel.touchMode;
  SessionID get sessionId => ffi.sessionId;

  /// Whether an action is a one-shot action (fires once, not continuously).
  bool _isOneShotAction(GestureAction action) {
    switch (action) {
      case GestureAction.leftClick:
      case GestureAction.rightClick:
      case GestureAction.doubleClick:
      case GestureAction.middleClick:
      case GestureAction.copy:
      case GestureAction.paste:
      case GestureAction.selectAll:
      case GestureAction.undo:
      case GestureAction.redo:
      case GestureAction.openKeyboard:
        return true;
      default:
        return false;
    }
  }

  /// Dispatch a tap-type gesture action (used by configurable gesture mapping).
  Future<void> _dispatchTapAction(GestureAction action) async {
    switch (action) {
      case GestureAction.leftClick:
        await inputModel.tap(MouseButtons.left);
        break;
      case GestureAction.rightClick:
        await inputModel.tap(MouseButtons.right);
        break;
      case GestureAction.doubleClick:
        await inputModel.tap(MouseButtons.left);
        await inputModel.tap(MouseButtons.left);
        break;
      case GestureAction.middleClick:
        await inputModel.tap(MouseButtons.wheel);
        break;
      case GestureAction.copy:
        inputModel.ctrl = true;
        inputModel.inputKey('VK_C');
        inputModel.ctrl = false;
        break;
      case GestureAction.paste:
        inputModel.ctrl = true;
        inputModel.inputKey('VK_V');
        inputModel.ctrl = false;
        break;
      case GestureAction.selectAll:
        inputModel.ctrl = true;
        inputModel.inputKey('VK_A');
        inputModel.ctrl = false;
        break;
      case GestureAction.undo:
        inputModel.ctrl = true;
        inputModel.inputKey('VK_Z');
        inputModel.ctrl = false;
        break;
      case GestureAction.redo:
        inputModel.ctrl = true;
        inputModel.inputKey('VK_Y');
        inputModel.ctrl = false;
        break;
      case GestureAction.openKeyboard:
        widget.onOpenKeyboard?.call();
        break;
      default:
        break;
    }
  }

  @override
  Widget build(BuildContext context) {
    return RawGestureDetector(
      child: widget.child,
      gestures: makeGestures(context),
    );
  }

  bool isNotTouchBasedDevice() {
    return !kTouchBasedDeviceKinds.contains(lastDeviceKind);
  }

  // Mobile, mouse mode.
  // Check if should block the mouse tap event (`_lastTapDownPositionForMouseMode`).
  bool shouldBlockMouseModeEvent() {
    return _lastTapDownPositionForMouseMode != null &&
        ffi.cursorModel.shouldBlock(_lastTapDownPositionForMouseMode!.dx,
            _lastTapDownPositionForMouseMode!.dy);
  }

  onTapDown(TapDownDetails d) async {
    widget.onInteraction?.call();
    lastDeviceKind = d.kind;
    _lastTapDownGlobalPosition = d.globalPosition;
    if (isNotTouchBasedDevice()) {
      return;
    }
    if (handleTouch) {
      _lastPosOfDoubleTapDown = d.localPosition;
      // Desktop or mobile "Touch mode"
      _lastTapDownDetails = d;
    } else {
      _lastTapDownPositionForMouseMode = d.localPosition;
    }
  }

  onTapUp(TapUpDetails d) async {
    final TapDownDetails? lastTapDownDetails = _lastTapDownDetails;
    _lastTapDownDetails = null;
    if (isNotTouchBasedDevice()) {
      return;
    }
    // Filter duplicate touch tap events on iOS (Magic Mouse issue).
    if (inputModel.shouldIgnoreTouchTap(d.globalPosition)) {
      return;
    }
    if (handleTouch) {
      final isMoved =
          await ffi.cursorModel.move(d.localPosition.dx, d.localPosition.dy);
      if (isMoved) {
        final action = GestureMapModel.getAction(true, GestureInput.tap1);
        if (action == GestureAction.doubleClick) {
          await inputModel.tap(MouseButtons.left);
          await inputModel.tap(MouseButtons.left);
        } else {
          final btn = action == GestureAction.rightClick
              ? MouseButtons.right
              : MouseButtons.left;
          if (lastTapDownDetails != null && !_touchModePanStarted) {
            await inputModel.tapDown(btn);
          }
          await inputModel.tapUp(btn);
        }
      }
    }
  }

  onTap() async {
    if (isNotTouchBasedDevice()) {
      return;
    }
    // Filter duplicate touch tap events on iOS (Magic Mouse issue).
    final lastPos = _lastTapDownGlobalPosition;
    if (lastPos != null && inputModel.shouldIgnoreTouchTap(lastPos)) {
      return;
    }
    if (!handleTouch) {
      // Cannot use `_lastTapDownDetails` because Flutter calls `onTapUp` before `onTap`, clearing the cached details.
      // Using `_lastTapDownPositionForMouseMode` instead.
      if (shouldBlockMouseModeEvent()) {
        return;
      }
      // Mobile, "Mouse mode" — configurable via gesture mapping
      await _dispatchTapAction(GestureMapModel.getAction(false, GestureInput.tap1));
    }
  }

  onDoubleTapDown(TapDownDetails d) async {
    widget.onInteraction?.call();
    lastDeviceKind = d.kind;
    if (isNotTouchBasedDevice()) {
      return;
    }
    if (handleTouch) {
      _lastPosOfDoubleTapDown = d.localPosition;
      await ffi.cursorModel.move(d.localPosition.dx, d.localPosition.dy);
    } else {
      _lastTapDownPositionForMouseMode = d.localPosition;
    }
  }

  onDoubleTap() async {
    if (isNotTouchBasedDevice()) {
      return;
    }
    if (ffiModel.touchMode && ffi.cursorModel.lastIsBlocked) {
      return;
    }
    if (handleTouch &&
        !ffi.cursorModel.isInRemoteRect(_lastPosOfDoubleTapDown)) {
      return;
    }
    // Check if the position is in a blocked area when using the mouse mode.
    if (!handleTouch) {
      if (shouldBlockMouseModeEvent()) {
        return;
      }
      await _dispatchTapAction(GestureMapModel.getAction(false, GestureInput.doubleTap));
      return;
    }
    await _dispatchTapAction(GestureMapModel.getAction(true, GestureInput.doubleTap));
  }

  onLongPressDown(LongPressDownDetails d) async {
    widget.onInteraction?.call();
    lastDeviceKind = d.kind;
    if (isNotTouchBasedDevice()) {
      return;
    }
    if (handleTouch) {
      _lastPosOfDoubleTapDown = d.localPosition;
      _cacheLongPressPosition = d.localPosition;
      if (!ffi.cursorModel.isInRemoteRect(d.localPosition)) {
        return;
      }
      _cacheLongPressPositionTs = DateTime.now().millisecondsSinceEpoch;
      if (ffiModel.isPeerMobile) {
        await ffi.cursorModel
            .move(_cacheLongPressPosition.dx, _cacheLongPressPosition.dy);
        await inputModel.tapDown(MouseButtons.left);
      }
    } else {
      _lastTapDownPositionForMouseMode = d.localPosition;
    }
  }

  onLongPressUp() async {
    if (isNotTouchBasedDevice()) {
      return;
    }
    if (handleTouch) {
      await inputModel.tapUp(MouseButtons.left);
    }
  }

  // for mobiles
  onLongPress() async {
    if (isNotTouchBasedDevice()) {
      return;
    }
    if (!ffi.ffiModel.isPeerMobile) {
      if (handleTouch) {
        final isMoved = await ffi.cursorModel
            .move(_cacheLongPressPosition.dx, _cacheLongPressPosition.dy);
        if (!isMoved) {
          return;
        }
      } else {
        if (shouldBlockMouseModeEvent()) {
          return;
        }
        await _dispatchTapAction(GestureMapModel.getAction(false, GestureInput.longPress));
        return;
      }
      await _dispatchTapAction(GestureMapModel.getAction(true, GestureInput.longPress));
    } else {
      // It's better to send a message to tell the controlled device that the long press event is triggered.
      // We're now using a `TimerTask` in `InputService.kt` to decide whether to trigger the long press event.
      // It's not accurate and it's better to use the same detection logic in the controlling side.
    }
  }

  onLongPressMoveUpdate(LongPressMoveUpdateDetails d) async {
    if (!ffiModel.isPeerMobile || isNotTouchBasedDevice()) {
      return;
    }
    if (handleTouch) {
      if (!ffi.cursorModel.isInRemoteRect(d.localPosition)) {
        return;
      }
      await ffi.cursorModel.move(d.localPosition.dx, d.localPosition.dy);
    }
  }

  onDoubleFinerTapDown(TapDownDetails d) async {
    widget.onInteraction?.call();
    lastDeviceKind = d.kind;
    if (isNotTouchBasedDevice()) {
      return;
    }
    _doubleFinerTapPosition = d.localPosition;
    // ignore for desktop and mobile
  }

  onDoubleFinerTap(TapDownDetails d) async {
    lastDeviceKind = d.kind;
    if (isNotTouchBasedDevice()) {
      return;
    }

    // mobile mouse mode or desktop touch screen
    final isMobileMouseMode = isMobile && !ffiModel.touchMode;
    // We can't use `d.localPosition` here because it's always (0, 0) on desktop.
    final isDesktopInRemoteRect = (isDesktop || isWebDesktop) &&
        ffi.cursorModel.isInRemoteRect(_doubleFinerTapPosition);
    if (isMobileMouseMode) {
      await _dispatchTapAction(GestureMapModel.getAction(false, GestureInput.tap2));
    } else if (isDesktopInRemoteRect) {
      await inputModel.tap(MouseButtons.right);
    }
  }

  onHoldDragStart(DragStartDetails d) async {
    widget.onInteraction?.call();
    lastDeviceKind = d.kind;
    if (isNotTouchBasedDevice()) {
      return;
    }
    if (handleTouch) {
      _activeHoldDragAction = GestureMapModel.getAction(true, GestureInput.holdDrag);
      _holdDragScrollIntegral = 0;

      // One-shot actions: dispatch immediately, no drag
      if (_isOneShotAction(_activeHoldDragAction!)) {
        await _dispatchTapAction(_activeHoldDragAction!);
        _activeHoldDragAction = null;
        return;
      }

      // Continuous actions that need mouse down
      if (_activeHoldDragAction == GestureAction.textSelection ||
          _activeHoldDragAction == GestureAction.drag) {
        await ffi.cursorModel.move(
            _lastPosOfDoubleTapDown.dx, _lastPosOfDoubleTapDown.dy);
        await inputModel.sendMouse('down', MouseButtons.left);
      }
    } else {
      if (isSpecialHoldDragActive) return;
      _activeHoldDragAction = GestureAction.drag;
      await inputModel.sendMouse('down', MouseButtons.left);
    }
  }

  onHoldDragUpdate(DragUpdateDetails d) async {
    if (isNotTouchBasedDevice()) {
      return;
    }
    if (_activeHoldDragAction == null) return;
    if (handleTouch) {
      switch (_activeHoldDragAction) {
        case GestureAction.textSelection:
        case GestureAction.drag:
        case GestureAction.moveCursor:
          await ffi.cursorModel.updatePan(d.delta, d.localPosition, handleTouch);
          break;
        case GestureAction.scroll:
          _holdDragScrollIntegral += d.delta.dy / 4;
          if (_holdDragScrollIntegral > 1) {
            inputModel.scroll(1);
            _holdDragScrollIntegral = 0;
          } else if (_holdDragScrollIntegral < -1) {
            inputModel.scroll(-1);
            _holdDragScrollIntegral = 0;
          }
          break;
        case GestureAction.panCanvas:
          ffi.canvasModel.panX(d.delta.dx);
          ffi.canvasModel.panY(d.delta.dy);
          break;
        default:
          break;
      }
    } else {
      if (isSpecialHoldDragActive) return;
      await ffi.cursorModel.updatePan(d.delta, d.localPosition, handleTouch);
    }
  }

  onHoldDragEnd(DragEndDetails d) async {
    if (isNotTouchBasedDevice()) {
      return;
    }
    final action = _activeHoldDragAction;
    _activeHoldDragAction = null;
    if (action == GestureAction.textSelection ||
        action == GestureAction.drag) {
      await inputModel.sendMouse('up', MouseButtons.left);
    } else if (!handleTouch) {
      await inputModel.sendMouse('up', MouseButtons.left);
    }
  }

  onOneFingerPanStart(BuildContext context, DragStartDetails d) async {
    widget.onInteraction?.call();
    final TapDownDetails? lastTapDownDetails = _lastTapDownDetails;
    _lastTapDownDetails = null;
    lastDeviceKind = d.kind ?? lastDeviceKind;
    if (isNotTouchBasedDevice()) {
      return;
    }
    if (handleTouch) {
      if (lastTapDownDetails != null) {
        await ffi.cursorModel.move(lastTapDownDetails.localPosition.dx,
            lastTapDownDetails.localPosition.dy);
      }
      if (ffi.cursorModel.shouldBlock(d.localPosition.dx, d.localPosition.dy)) {
        return;
      }
      if (!ffi.cursorModel.isInRemoteRect(d.localPosition)) {
        return;
      }

      _touchModePanStarted = true;
      _oneFingerScrollIntegral = 0;
      if (isDesktop || isWebDesktop) {
        ffi.cursorModel.trySetRemoteWindowCoords();
      }

      _activeOneFingerPanAction =
          GestureMapModel.getAction(true, GestureInput.pan1);

      // One-shot actions: dispatch immediately, don't start pan
      if (_isOneShotAction(_activeOneFingerPanAction!)) {
        await _dispatchTapAction(_activeOneFingerPanAction!);
        _activeOneFingerPanAction = null;
        _touchModePanStarted = false;
        return;
      }

      // Workaround for the issue that the first pan event is sent a long time after the start event.
      if (DateTime.now().millisecondsSinceEpoch - _cacheLongPressPositionTs <
          500) {
        await ffi.cursorModel
            .move(_cacheLongPressPosition.dx, _cacheLongPressPosition.dy);
      }
      // Only send mouse down for drag action
      if (_activeOneFingerPanAction == GestureAction.drag) {
        if (!inputModel.relativeMouseMode.value) {
          await inputModel.sendMouse('down', MouseButtons.left);
        }
      }
      if (_activeOneFingerPanAction == GestureAction.drag ||
          _activeOneFingerPanAction == GestureAction.moveCursor) {
        await ffi.cursorModel.move(d.localPosition.dx, d.localPosition.dy);
      }
    } else {
      final offset = ffi.cursorModel.offset;
      final cursorX = offset.dx;
      final cursorY = offset.dy;
      final visible =
          ffi.cursorModel.getVisibleRect().inflate(1); // extend edges
      final size = MediaQueryData.fromView(View.of(context)).size;
      if (!visible.contains(Offset(cursorX, cursorY))) {
        await ffi.cursorModel.move(size.width / 2, size.height / 2);
      }
    }
  }

  onOneFingerPanUpdate(DragUpdateDetails d) async {
    if (isNotTouchBasedDevice()) {
      return;
    }
    if (ffi.cursorModel.shouldBlock(d.localPosition.dx, d.localPosition.dy)) {
      return;
    }
    if (handleTouch && !_touchModePanStarted) {
      return;
    }
    if (handleTouch) {
      // Dispatch based on configured one-finger pan action
      switch (_activeOneFingerPanAction) {
        case GestureAction.scroll:
          _oneFingerScrollIntegral += d.delta.dy / 4;
          if (_oneFingerScrollIntegral > 1) {
            inputModel.scroll(1);
            _oneFingerScrollIntegral = 0;
          } else if (_oneFingerScrollIntegral < -1) {
            inputModel.scroll(-1);
            _oneFingerScrollIntegral = 0;
          }
          return;
        case GestureAction.panCanvas:
          ffi.canvasModel.panX(d.delta.dx);
          ffi.canvasModel.panY(d.delta.dy);
          return;
        default:
          // drag / moveCursor — original behavior
          break;
      }
    }
    // In relative mouse mode, send delta directly without position tracking.
    if (inputModel.relativeMouseMode.value) {
      await inputModel.sendMobileRelativeMouseMove(d.delta.dx, d.delta.dy);
    } else {
      await ffi.cursorModel.updatePan(d.delta, d.localPosition, handleTouch);
    }
  }

  onOneFingerPanEnd(DragEndDetails d) async {
    final panAction = _activeOneFingerPanAction;
    _touchModePanStarted = false;
    _activeOneFingerPanAction = null;
    if (isNotTouchBasedDevice()) {
      return;
    }
    if (isDesktop || isWebDesktop) {
      ffi.cursorModel.clearRemoteWindowCoords();
    }
    if (handleTouch) {
      // Only send mouse up for drag action (matches mouse down in onOneFingerPanStart)
      if (panAction == GestureAction.drag) {
        if (!inputModel.relativeMouseMode.value) {
          await inputModel.sendMouse('up', MouseButtons.left);
        }
      }
    }
  }

  // Reset `_touchModePanStarted` if the one-finger pan gesture is cancelled
  // or rejected by the gesture arena. Without this, the flag can remain
  // stuck in the "started" state and cause issues such as the Magic Mouse
  // double-click problem on iPad with magic mouse.
  onOneFingerPanCancel() {
    _touchModePanStarted = false;
  }

  // scale + pan event
  onTwoFingerScaleStart(ScaleStartDetails d) {
    widget.onInteraction?.call();
    _lastTapDownDetails = null;
    if (isNotTouchBasedDevice()) {
      return;
    }
    _twoFingerIsPinch = null; // Reset intent for new gesture
    _twoFingerOneShotFired = false;
    _mouseScrollIntegral = 0;
    _twoFingerCumulativeScale = 0;
    _twoFingerCumulativeTranslation = 0;
    if (isSpecialHoldDragActive) {
      // Initialize the last focal point to calculate deltas manually.
      _lastSpecialHoldDragFocalPoint = d.focalPoint;
    } else if (isMobile) {
      // Dispatch one-shot pan2 actions at gesture start
      final pan2Action = GestureMapModel.getAction(
          ffiModel.touchMode, GestureInput.pan2);
      if (_isOneShotAction(pan2Action)) {
        _dispatchTapAction(pan2Action);
        _twoFingerOneShotFired = true;
      }
    }
  }

  onTwoFingerScaleUpdate(ScaleUpdateDetails d) async {
    if (isNotTouchBasedDevice()) {
      return;
    }

    // If in special drag mode, perform a pan instead of a scale.
    if (isSpecialHoldDragActive) {
      // Calculate delta manually to avoid the jumpy behavior.
      final delta = d.focalPoint - _lastSpecialHoldDragFocalPoint;
      _lastSpecialHoldDragFocalPoint = d.focalPoint;
      await ffi.cursorModel.updatePan(delta * 2.0, d.focalPoint, handleTouch);
      return;
    }

    if ((isDesktop || isWebDesktop)) {
      final scale = ((d.scale - _scale) * 1000).toInt();
      _scale = d.scale;

      if (scale != 0) {
        if (widget.isCamera) return;
        await bind.sessionSendPointer(
            sessionId: sessionId,
            msg: json.encode(
                PointerEventToRust(kPointerEventKindTouch, 'scale', scale)
                    .toJson()));
      }
    } else {
      // mobile — cumulative intent detection for pinch vs pan2
      // Inspired by iOS/Android gesture recognizers: accumulate both
      // scale change and translation, first to reach threshold wins.
      if (_twoFingerOneShotFired) {
        _scale = d.scale;
        return;
      }

      final pan2Action = GestureMapModel.getAction(
          ffiModel.touchMode, GestureInput.pan2);
      final pinchAction = GestureMapModel.getAction(
          ffiModel.touchMode, GestureInput.pinch);

      // Accumulate evidence for both gestures.
      // Deadzone: ignore scale jitter < 0.003 per frame (sensor noise).
      final scaleDelta = (d.scale - _scale).abs();
      if (scaleDelta > 0.003) {
        _twoFingerCumulativeScale += scaleDelta;
      }
      _twoFingerCumulativeTranslation += d.focalPointDelta.distance;

      // Thresholds for intent determination.
      // Scale threshold is intentionally high: requires real pinch movement.
      // Translation threshold is low: parallel finger movement wins easily.
      const kScaleThreshold = 0.12;
      const kTranslationThreshold = 10.0;

      if (_twoFingerIsPinch == null) {
        // Race: first signal to reach its threshold determines intent.
        // Translation is checked first (bias towards scroll/pan).
        if (_twoFingerCumulativeTranslation >= kTranslationThreshold) {
          _twoFingerIsPinch = false;
        } else if (_twoFingerCumulativeScale >= kScaleThreshold) {
          _twoFingerIsPinch = true;
        }
        // Still undetermined — track scale but don't act yet
        _scale = d.scale;
        return;
      }

      if (_twoFingerIsPinch!) {
        // Pinch-to-zoom
        if (pinchAction == GestureAction.zoomCanvas) {
          ffi.canvasModel.updateScale(d.scale / _scale, d.focalPoint);
        }
      } else {
        // Pan2 action
        if (pan2Action == GestureAction.scroll) {
          _mouseScrollIntegral += d.focalPointDelta.dy / 4;
          if (_mouseScrollIntegral > 1) {
            inputModel.scroll(1);
            _mouseScrollIntegral = 0;
          } else if (_mouseScrollIntegral < -1) {
            inputModel.scroll(-1);
            _mouseScrollIntegral = 0;
          }
        } else if (pan2Action == GestureAction.panCanvas) {
          ffi.canvasModel.panX(d.focalPointDelta.dx);
          ffi.canvasModel.panY(d.focalPointDelta.dy);
        }
      }
      _scale = d.scale;
    }
  }

  onTwoFingerScaleEnd(ScaleEndDetails d) async {
    if (isNotTouchBasedDevice()) {
      return;
    }
    _twoFingerIsPinch = null;
    if ((isDesktop || isWebDesktop)) {
      if (widget.isCamera) return;
      await bind.sessionSendPointer(
          sessionId: sessionId,
          msg: json.encode(
              PointerEventToRust(kPointerEventKindTouch, 'scale', 0).toJson()));
    } else {
      // mobile
      _scale = 1;
    }
    if (!isSpecialHoldDragActive) {
      await inputModel.sendMouse('up', MouseButtons.left);
    }
  }

  get onHoldDragCancel {
    _activeHoldDragAction = null;
    return null;
  }

  get onThreeFingerVerticalDragStart => ffi.ffiModel.isPeerAndroid
      ? null
      : (DragStartDetails d) {
          _threeFingerOneShotFired = false;
          final action = GestureMapModel.getAction(
              ffiModel.touchMode, GestureInput.pan3);
          if (_isOneShotAction(action)) {
            _dispatchTapAction(action);
            _threeFingerOneShotFired = true;
          }
        };

  get onThreeFingerVerticalDragUpdate => ffi.ffiModel.isPeerAndroid
      ? null
      : (d) {
          if (_threeFingerOneShotFired) return;
          final action = GestureMapModel.getAction(
              ffiModel.touchMode, GestureInput.pan3);
          if (action == GestureAction.scroll) {
            _mouseScrollIntegral += d.delta.dy / 4;
            if (_mouseScrollIntegral > 1) {
              inputModel.scroll(1);
              _mouseScrollIntegral = 0;
            } else if (_mouseScrollIntegral < -1) {
              inputModel.scroll(-1);
              _mouseScrollIntegral = 0;
            }
          } else if (action == GestureAction.panCanvas) {
            ffi.canvasModel.panX(d.delta.dx);
            ffi.canvasModel.panY(d.delta.dy);
          }
        };

  makeGestures(BuildContext context) {
    return <Type, GestureRecognizerFactory>{
      // Official
      TapGestureRecognizer:
          GestureRecognizerFactoryWithHandlers<TapGestureRecognizer>(
              () => TapGestureRecognizer(), (instance) {
        instance
          ..onTapDown = onTapDown
          ..onTapUp = onTapUp
          ..onTap = onTap;
      }),
      DoubleTapGestureRecognizer:
          GestureRecognizerFactoryWithHandlers<DoubleTapGestureRecognizer>(
              () => DoubleTapGestureRecognizer(), (instance) {
        instance
          ..onDoubleTapDown = onDoubleTapDown
          ..onDoubleTap = onDoubleTap;
      }),
      LongPressGestureRecognizer:
          GestureRecognizerFactoryWithHandlers<LongPressGestureRecognizer>(
              () => LongPressGestureRecognizer(), (instance) {
        instance
          ..onLongPressDown = onLongPressDown
          ..onLongPressUp = onLongPressUp
          ..onLongPress = onLongPress
          ..onLongPressMoveUpdate = onLongPressMoveUpdate;
      }),
      // Customized
      HoldTapMoveGestureRecognizer:
          GestureRecognizerFactoryWithHandlers<HoldTapMoveGestureRecognizer>(
              () => HoldTapMoveGestureRecognizer(),
              (instance) => instance
                ..onHoldDragStart = onHoldDragStart
                ..onHoldDragUpdate = onHoldDragUpdate
                ..onHoldDragCancel = onHoldDragCancel
                ..onHoldDragEnd = onHoldDragEnd),
      DoubleFinerTapGestureRecognizer:
          GestureRecognizerFactoryWithHandlers<DoubleFinerTapGestureRecognizer>(
              () => DoubleFinerTapGestureRecognizer(), (instance) {
        instance
          ..onDoubleFinerTap = onDoubleFinerTap
          ..onDoubleFinerTapDown = onDoubleFinerTapDown;
      }),
      CustomTouchGestureRecognizer:
          GestureRecognizerFactoryWithHandlers<CustomTouchGestureRecognizer>(
              () => CustomTouchGestureRecognizer(), (instance) {
        instance.onOneFingerPanStart =
            (DragStartDetails d) => onOneFingerPanStart(context, d);
        instance
          ..onOneFingerPanUpdate = onOneFingerPanUpdate
          ..onOneFingerPanEnd = onOneFingerPanEnd
          ..onOneFingerPanCancel = onOneFingerPanCancel
          ..onTwoFingerScaleStart = onTwoFingerScaleStart
          ..onTwoFingerScaleUpdate = onTwoFingerScaleUpdate
          ..onTwoFingerScaleEnd = onTwoFingerScaleEnd
          ..onThreeFingerVerticalDragStart = onThreeFingerVerticalDragStart
          ..onThreeFingerVerticalDragUpdate = onThreeFingerVerticalDragUpdate;
      }),
    };
  }
}

class RawPointerMouseRegion extends StatelessWidget {
  final InputModel inputModel;
  final Widget child;
  final MouseCursor? cursor;
  final PointerEnterEventListener? onEnter;
  final PointerExitEventListener? onExit;
  final PointerDownEventListener? onPointerDown;
  final PointerUpEventListener? onPointerUp;

  RawPointerMouseRegion({
    this.onEnter,
    this.onExit,
    this.cursor,
    this.onPointerDown,
    this.onPointerUp,
    required this.inputModel,
    required this.child,
  });

  @override
  Widget build(BuildContext context) {
    return Listener(
      onPointerHover: inputModel.onPointHoverImage,
      onPointerDown: (evt) {
        onPointerDown?.call(evt);
        inputModel.onPointDownImage(evt);
      },
      onPointerUp: (evt) {
        onPointerUp?.call(evt);
        inputModel.onPointUpImage(evt);
      },
      onPointerMove: inputModel.onPointMoveImage,
      onPointerSignal: inputModel.onPointerSignalImage,
      onPointerPanZoomStart: inputModel.onPointerPanZoomStart,
      onPointerPanZoomUpdate: inputModel.onPointerPanZoomUpdate,
      onPointerPanZoomEnd: inputModel.onPointerPanZoomEnd,
      child: MouseRegion(
        cursor: inputModel.isViewOnly
            ? MouseCursor.defer
            : (cursor ?? MouseCursor.defer),
        onEnter: onEnter,
        onExit: onExit,
        child: child,
      ),
    );
  }
}

class CameraRawPointerMouseRegion extends StatelessWidget {
  final InputModel inputModel;
  final Widget child;
  final PointerEnterEventListener? onEnter;
  final PointerExitEventListener? onExit;
  final PointerDownEventListener? onPointerDown;
  final PointerUpEventListener? onPointerUp;

  CameraRawPointerMouseRegion({
    this.onEnter,
    this.onExit,
    this.onPointerDown,
    this.onPointerUp,
    required this.inputModel,
    required this.child,
  });

  @override
  Widget build(BuildContext context) {
    return Listener(
      onPointerHover: (evt) {
        final offset = evt.position;
        double x = offset.dx;
        double y = max(0.0, offset.dy);
        inputModel.handlePointerDevicePos(
            kPointerEventKindMouse, x, y, true, kMouseEventTypeDefault);
      },
      onPointerDown: (evt) {
        onPointerDown?.call(evt);
      },
      onPointerUp: (evt) {
        onPointerUp?.call(evt);
      },
      child: MouseRegion(
        cursor: MouseCursor.defer,
        onEnter: onEnter,
        onExit: onExit,
        child: child,
      ),
    );
  }
}
