import 'package:flutter/material.dart';
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/consts.dart';
import 'package:flutter_hbb/models/gesture_map_model.dart';
import 'package:flutter_hbb/models/input_model.dart';
import 'package:flutter_hbb/models/model.dart';
import 'package:flutter_hbb/models/platform_model.dart';
import 'package:get/get.dart';
import 'package:toggle_switch/toggle_switch.dart';

class GestureIcons {
  static const String _family = 'gestureicons';

  GestureIcons._();

  static const IconData iconMouse = IconData(0xe65c, fontFamily: _family);
  static const IconData iconTabletTouch = IconData(0xe9ce, fontFamily: _family);
  static const IconData iconGestureFDrag =
      IconData(0xe686, fontFamily: _family);
  static const IconData iconMobileTouch = IconData(0xe9cd, fontFamily: _family);
  static const IconData iconGesturePress =
      IconData(0xe66c, fontFamily: _family);
  static const IconData iconGestureTap = IconData(0xe66f, fontFamily: _family);
  static const IconData iconGesturePinch =
      IconData(0xe66a, fontFamily: _family);
  static const IconData iconGesturePressHold =
      IconData(0xe66b, fontFamily: _family);
  static const IconData iconGestureFDragUpDown_ =
      IconData(0xe685, fontFamily: _family);
  static const IconData iconGestureFTap_ =
      IconData(0xe68e, fontFamily: _family);
  static const IconData iconGestureFSwipeRight =
      IconData(0xe68f, fontFamily: _family);
  static const IconData iconGestureFdoubleTap =
      IconData(0xe691, fontFamily: _family);
  static const IconData iconGestureFThreeFingers =
      IconData(0xe687, fontFamily: _family);
}

typedef OnTouchModeChange = void Function(bool);

class GestureHelp extends StatefulWidget {
  GestureHelp(
      {Key? key,
      required this.touchMode,
      required this.onTouchModeChange,
      required this.virtualMouseMode,
      this.inputModel})
      : super(key: key);
  final bool touchMode;
  final OnTouchModeChange onTouchModeChange;
  final VirtualMouseMode virtualMouseMode;
  final InputModel? inputModel;

  @override
  State<StatefulWidget> createState() =>
      _GestureHelpState(touchMode, virtualMouseMode);
}

class _GestureHelpState extends State<GestureHelp> {
  late int _selectedIndex;
  late bool _touchMode;
  final VirtualMouseMode _virtualMouseMode;
  bool _showAllGestures = false;

  _GestureHelpState(bool touchMode, VirtualMouseMode virtualMouseMode)
      : _virtualMouseMode = virtualMouseMode {
    _touchMode = touchMode;
    _selectedIndex = _touchMode ? 1 : 0;
  }

  /// Helper to exit relative mouse mode when certain conditions are met.
  void _exitRelativeMouseModeIf(bool condition) {
    if (condition) {
      widget.inputModel?.setRelativeMouseMode(false);
    }
  }

  void _showActionPicker(GestureInput input, GestureAction currentAction) {
    showDialog(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(translate(
            GestureMapModel.cardInputLabels[input] ?? input.name)),
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

  Widget _buildCursorCheckbox({required String key, required String label}) {
    return Transform.translate(
      offset: const Offset(-10.0, 0.0),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Checkbox(
            value: bind.mainGetLocalOption(key: key) == 'Y',
            onChanged: (value) async {
              if (value == null) return;
              await bind.mainSetLocalOption(
                key: key,
                value: value ? 'Y' : '',
              );
              setState(() {});
            },
          ),
          Flexible(
            child: InkWell(
              onTap: () async {
                final current = bind.mainGetLocalOption(key: key) == 'Y';
                await bind.mainSetLocalOption(
                  key: key,
                  value: !current ? 'Y' : '',
                );
                setState(() {});
              },
              child: Text(translate(label),
                  overflow: TextOverflow.ellipsis, maxLines: 1),
            ),
          ),
        ],
      ),
    );
  }

  List<Widget> _buildGestureCards(double width) {
    final defaultInputs = _touchMode
        ? GestureMapModel.displayedTouchInputs
        : GestureMapModel.displayedMouseInputs;

    final inputs = _showAllGestures
        ? GestureInput.values
        : defaultInputs;

    final cards = inputs.map((input) {
      final action = GestureMapModel.getAction(_touchMode, input);
      final isConfigurable =
          GestureMapModel.configurableInputs.contains(input);
      final isCustom = !GestureMapModel.isDefault(_touchMode, input);
      final actionLabel =
          translate(gestureActionLabels[action] ?? action.name);
      final inputLabel = translate(
          GestureMapModel.cardInputLabels[input] ?? input.name);
      final icon = GestureMapModel.iconForInput(input);

      return GestureInfo(
        width: width,
        icon: icon,
        fromText: inputLabel,
        toText: actionLabel,
        isCustom: isCustom,
        onTap: isConfigurable
            ? () => _showActionPicker(input, action)
            : null,
      );
    }).toList();

    // Add +/- toggle card
    cards.add(GestureInfo(
      width: width,
      icon: _showAllGestures ? Icons.remove_circle_outline : Icons.add_circle_outline,
      fromText: '',
      toText: _showAllGestures
          ? translate('Show less')
          : translate('Show more'),
      onTap: () => setState(() => _showAllGestures = !_showAllGestures),
    ));

    return cards;
  }

  @override
  Widget build(BuildContext context) {
    final size = MediaQuery.of(context).size;
    final space = 12.0;
    var width = size.width - 2 * space;
    final minWidth = 90;
    if (size.width > minWidth + 2 * space) {
      final n = (size.width / (minWidth + 2 * space)).floor();
      width = size.width / n - 2 * space;
    }
    return Center(
        child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 12.0),
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: <Widget>[
                Center(
                  child: Column(
                    children: [
                      ToggleSwitch(
                        initialLabelIndex: _selectedIndex,
                        activeFgColor: Colors.white,
                        inactiveFgColor: Colors.white60,
                        activeBgColor: [MyTheme.accent],
                        inactiveBgColor: Theme.of(context).hintColor,
                        totalSwitches: 2,
                        minWidth: 150,
                        fontSize: 15,
                        iconSize: 18,
                        labels: [
                          translate("Mouse mode"),
                          translate("Touch mode")
                        ],
                        icons: [Icons.mouse, Icons.touch_app],
                        onToggle: (index) {
                          setState(() {
                            if (_selectedIndex != index) {
                              _selectedIndex = index ?? 0;
                              _touchMode = index == 0 ? false : true;
                              widget.onTouchModeChange(_touchMode);
                              _exitRelativeMouseModeIf(_touchMode);
                            }
                          });
                        },
                      ),
                      const SizedBox(height: 8),
                      Transform.translate(
                        offset: const Offset(-10.0, 0.0),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Checkbox(
                              value: _virtualMouseMode.showVirtualMouse,
                              onChanged: (value) async {
                                if (value == null) return;
                                await _virtualMouseMode.toggleVirtualMouse();
                                _exitRelativeMouseModeIf(
                                    !_virtualMouseMode.showVirtualMouse);
                                setState(() {});
                              },
                            ),
                            Flexible(
                              child: InkWell(
                                onTap: () async {
                                  await _virtualMouseMode.toggleVirtualMouse();
                                  _exitRelativeMouseModeIf(
                                      !_virtualMouseMode.showVirtualMouse);
                                  setState(() {});
                                },
                                child: Text(translate('Show virtual mouse'),
                                    overflow: TextOverflow.ellipsis,
                                    maxLines: 1),
                              ),
                            ),
                          ],
                        ),
                      ),
                      if (_touchMode && _virtualMouseMode.showVirtualMouse)
                        Padding(
                          padding: const EdgeInsets.only(left: 24.0),
                          child: SizedBox(
                            width: 260,
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Padding(
                                  padding: const EdgeInsets.only(
                                      top: 0.0, bottom: 0),
                                  child: Text(translate('Virtual mouse size')),
                                ),
                                Transform.translate(
                                  offset: Offset(-0.0, -6.0),
                                  child: Row(
                                    children: [
                                      Padding(
                                        padding:
                                            const EdgeInsets.only(left: 0.0),
                                        child: Text(translate('Small')),
                                      ),
                                      Expanded(
                                        child: Slider(
                                          value: _virtualMouseMode
                                              .virtualMouseScale,
                                          min: 0.8,
                                          max: 1.8,
                                          divisions: 10,
                                          onChanged: (value) {
                                            _virtualMouseMode
                                                .setVirtualMouseScale(value);
                                            setState(() {});
                                          },
                                        ),
                                      ),
                                      Padding(
                                        padding:
                                            const EdgeInsets.only(right: 16.0),
                                        child: Text(translate('Large')),
                                      ),
                                    ],
                                  ),
                                ),
                              ],
                            ),
                          ),
                        ),
                      if (!_touchMode && _virtualMouseMode.showVirtualMouse)
                        Transform.translate(
                          offset: const Offset(-10.0, -12.0),
                          child: Padding(
                              padding: const EdgeInsets.only(left: 24.0),
                              child: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  Checkbox(
                                    value:
                                        _virtualMouseMode.showVirtualJoystick,
                                    onChanged: (value) async {
                                      if (value == null) return;
                                      await _virtualMouseMode
                                          .toggleVirtualJoystick();
                                      _exitRelativeMouseModeIf(
                                          !_virtualMouseMode
                                              .showVirtualJoystick);
                                      setState(() {});
                                    },
                                  ),
                                  Flexible(
                                    child: InkWell(
                                      onTap: () async {
                                        await _virtualMouseMode
                                            .toggleVirtualJoystick();
                                        _exitRelativeMouseModeIf(
                                            !_virtualMouseMode
                                                .showVirtualJoystick);
                                        setState(() {});
                                      },
                                      child: Text(
                                          translate("Show virtual joystick"),
                                          overflow: TextOverflow.ellipsis,
                                          maxLines: 1),
                                    ),
                                  ),
                                ],
                              )),
                        ),
                      // Relative mouse mode option
                      if (!_touchMode &&
                          _virtualMouseMode.showVirtualMouse &&
                          _virtualMouseMode.showVirtualJoystick &&
                          widget.inputModel != null)
                        Obx(() => Transform.translate(
                              offset: const Offset(-10.0, -24.0),
                              child: Padding(
                                  padding: const EdgeInsets.only(left: 48.0),
                                  child: Row(
                                    mainAxisSize: MainAxisSize.min,
                                    children: [
                                      Checkbox(
                                        value: widget.inputModel!
                                            .relativeMouseMode.value,
                                        onChanged: (value) {
                                          if (value == null) return;
                                          widget.inputModel!
                                              .setRelativeMouseMode(value);
                                        },
                                      ),
                                      Flexible(
                                        child: InkWell(
                                          onTap: () {
                                            widget.inputModel!
                                                .toggleRelativeMouseMode();
                                          },
                                          child: Text(
                                              translate('Relative mouse mode'),
                                              overflow: TextOverflow.ellipsis,
                                              maxLines: 1),
                                        ),
                                      ),
                                    ],
                                  )),
                            )),
                      Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Flexible(
                            child: _buildCursorCheckbox(
                              key: _touchMode
                                  ? kOptionHideLocalCursorTouch
                                  : kOptionHideLocalCursorMouse,
                              label: 'Hide local cursor',
                            ),
                          ),
                          Flexible(
                            child: _buildCursorCheckbox(
                              key: _touchMode
                                  ? kOptionHideRemoteCursorTouch
                                  : kOptionHideRemoteCursorMouse,
                              label: 'Hide distant cursor',
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: 4),
                // Reset to default button
                TextButton.icon(
                  icon: const Icon(Icons.refresh, size: 16),
                  label: Text(translate('Reset to default'),
                      style: const TextStyle(fontSize: 12)),
                  onPressed: () async {
                    await GestureMapModel.resetDefaults(_touchMode);
                    setState(() {});
                  },
                ),
                Container(
                    child: Wrap(
                  spacing: space,
                  runSpacing: 2 * space,
                  children: _buildGestureCards(width),
                )),
              ],
            )));
  }
}

class GestureInfo extends StatelessWidget {
  const GestureInfo({
    Key? key,
    required this.width,
    required this.icon,
    required this.fromText,
    required this.toText,
    this.isCustom = false,
    this.onTap,
  }) : super(key: key);

  final String fromText;
  final String toText;
  final IconData icon;
  final double width;
  final bool isCustom;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final accentColor = MyTheme.accent;
    final iconColor = isCustom ? accentColor : accentColor;
    const iconSize = 35.0;

    final content = Container(
        width: width,
        child: Column(
          children: [
            Stack(
              alignment: Alignment.topRight,
              children: [
                Icon(icon, size: iconSize, color: iconColor),
                if (onTap != null)
                  Icon(Icons.edit, size: 10,
                      color: Theme.of(context).hintColor.withOpacity(0.6)),
              ],
            ),
            SizedBox(height: 6),
            Text(fromText,
                textAlign: TextAlign.center,
                overflow: TextOverflow.ellipsis,
                maxLines: 1,
                style: TextStyle(
                    fontSize: 9, color: Theme.of(context).hintColor)),
            SizedBox(height: 3),
            Text(toText,
                textAlign: TextAlign.center,
                overflow: TextOverflow.ellipsis,
                maxLines: 1,
                style: TextStyle(
                  fontSize: 12,
                  fontWeight: isCustom ? FontWeight.bold : FontWeight.normal,
                  color: isCustom
                      ? accentColor
                      : Theme.of(context).textTheme.bodySmall?.color,
                )),
          ],
        ));

    if (onTap != null) {
      return InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(8),
        child: content,
      );
    }
    return content;
  }
}
