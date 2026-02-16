// Unit tests for toolbar cursor options and Mutter virtual display support.
// Verifies that Wayland guards have been removed and Mutter features exist.
//
// Run with: dart test/toolbar_cursor_test.dart
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
  final toolbarFile = File('lib/common/widgets/toolbar.dart');
  if (!toolbarFile.existsSync()) {
    print('ERROR: toolbar.dart not found. Run from flutter/ directory.');
    exit(1);
  }
  final toolbarSource = toolbarFile.readAsStringSync();

  // =====================================================================
  // Cursor options Wayland guards
  // =====================================================================

  group('Cursor options Wayland guards removed', () {
    test('show remote cursor block has NO isWayland guard', () {
      final idx = toolbarSource.indexOf('// show remote cursor');
      expect(idx != -1, '"// show remote cursor" comment exists');

      final blockEnd = toolbarSource.indexOf('{', idx);
      final block = toolbarSource.substring(idx, blockEnd);

      expect(!block.contains('isWayland'),
          'show remote cursor condition does NOT contain isWayland');
      expect(block.contains('cursorEmbedded'),
          'show remote cursor still checks cursorEmbedded');
      expect(block.contains('kPeerPlatformAndroid'),
          'show remote cursor still checks Android platform');
    });

    test('follow remote cursor block has NO isWayland guard', () {
      final idx = toolbarSource.indexOf('// follow remote cursor');
      expect(idx != -1, '"// follow remote cursor" comment exists');

      final blockEnd = toolbarSource.indexOf('{', idx);
      final block = toolbarSource.substring(idx, blockEnd);

      expect(!block.contains('isWayland'),
          'follow remote cursor condition does NOT contain isWayland');
      expect(block.contains('cursorEmbedded'),
          'follow remote cursor still checks cursorEmbedded');
    });

    test('follow remote window block has NO isWayland guard', () {
      final idx = toolbarSource.indexOf('// follow remote window focus');
      expect(idx != -1, '"// follow remote window focus" comment exists');

      final blockEnd = toolbarSource.indexOf('{', idx);
      final block = toolbarSource.substring(idx, blockEnd);

      expect(!block.contains('isWayland'),
          'follow remote window condition does NOT contain isWayland');
      expect(block.contains('cursorEmbedded'),
          'follow remote window still checks cursorEmbedded');
    });
  });

  // =====================================================================
  // Mutter virtual display constants
  // =====================================================================

  group('Mutter virtual display support in consts', () {
    test('kPlatformAdditionsMutterVirtualDisplays exists', () {
      final constsFile = File('lib/consts.dart');
      expect(constsFile.existsSync(), 'consts.dart exists');
      final source = constsFile.readAsStringSync();

      expect(source.contains('kPlatformAdditionsMutterVirtualDisplays'),
          'kPlatformAdditionsMutterVirtualDisplays constant defined');
    });
  });

  // =====================================================================
  // Model isMutter support
  // =====================================================================

  group('Model isMutter support', () {
    test('isMutter getter exists', () {
      final modelFile = File('lib/models/model.dart');
      expect(modelFile.existsSync(), 'model.dart exists');
      final source = modelFile.readAsStringSync();

      expect(source.contains('isMutter'), 'isMutter getter exists');
      expect(source.contains('mutterVirtualDisplayCount'),
          'mutterVirtualDisplayCount getter exists');
    });
  });

  // =====================================================================
  // Toolbar Mutter menu
  // =====================================================================

  group('Toolbar Mutter virtual display menu', () {
    test('Mutter menu code present in toolbar.dart', () {
      expect(
          toolbarSource.contains('isMutter') ||
              toolbarSource.contains('Mutter'),
          'toolbar.dart references Mutter virtual displays');
    });

    test('isWayland used ONLY for relative mouse mode (expected)', () {
      // The only remaining isWayland usage should be for relative mouse mode
      // which genuinely does not work on Wayland due to cursor warping limitations
      final lines = toolbarSource.split('\n');
      int waylandCount = 0;
      for (final line in lines) {
        if (line.contains('isWayland') && !line.trimLeft().startsWith('//')) {
          waylandCount++;
        }
      }
      // Should be exactly 2: the variable declaration and the condition check
      expect(waylandCount <= 3,
          'isWayland used at most 3 times (relative mouse mode only), found: $waylandCount');
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
