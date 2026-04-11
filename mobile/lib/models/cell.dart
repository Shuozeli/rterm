import 'dart:ui';

/// Cell flags matching rterm_core::cell::Flags bitfield layout.
class CellFlags {
  static const int inverse = 1 << 0;
  static const int bold = 1 << 1;
  static const int italic = 1 << 2;
  static const int underline = 1 << 3;
  static const int wrapLine = 1 << 4;
  static const int wideChar = 1 << 5;
  static const int wideCharSpacer = 1 << 6;
  static const int dim = 1 << 7;
  static const int hidden = 1 << 8;
  static const int strikethrough = 1 << 9;
  static const int leadingWideCharSpacer = 1 << 10;
  static const int doubleUnderline = 1 << 11;
  static const int undercurl = 1 << 12;
  static const int dottedUnderline = 1 << 13;
  static const int dashedUnderline = 1 << 14;
}

/// Color encoding matching rterm_core::color::Color.
/// - Lower 24 bits: RGB (0x00RRGGBB)
/// - Special values: 0xFFFFFFFF = Default
class CellColor {
  static const int defaultColor = 0xFFFFFFFF;

  final int value;

  const CellColor(this.value);

  bool get isDefault => value == defaultColor;

  /// Returns true if this is an indexed color (0x1RRGGBB where RRGGBB is palette index)
  bool get isIndexed => !isDefault && (value >> 24) == 0x1;

  /// Returns true if this is an RGB color (0x2RRGGBB)
  bool get isRgb => !isDefault && (value >> 24) == 0x2;

  /// Get RGB components for direct color (0x00RRGGBB)
  int get r => value & 0xFF;
  int get g => (value >> 8) & 0xFF;
  int get b => (value >> 16) & 0xFF;

  /// Get palette index for indexed color (0x1RRGGBB where RRGGBB is 0-255)
  int get paletteIndex => value & 0xFF;

  Color toColor() {
    if (isDefault) return const Color(0xFFFFFFFF);
    if (isIndexed) {
      final idx = paletteIndex;
      if (idx < _ansiColors.length) {
        return _ansiColors[idx];
      }
      return const Color(0xFFFFFFFF);
    }
    // RGB direct color
    return Color.fromARGB(255, r, g, b);
  }

  /// 16-color ANSI palette (matching xterm)
  static const List<Color> _ansiColors = [
    Color(0xFF000000), // 0: black
    Color(0xFFCD0000), // 1: red
    Color(0xFF00CD00), // 2: green
    Color(0xFFCDCD00), // 3: yellow
    Color(0xFF0000EE), // 4: blue
    Color(0xFFCD00CD), // 5: magenta
    Color(0xFF00CDCD), // 6: cyan
    Color(0xFFE5E5E5), // 7: white
    Color(0xFF7F7F7F), // 8: bright black
    Color(0xFFFF0000), // 9: bright red
    Color(0xFF00FF00), // 10: bright green
    Color(0xFFFFFF00), // 11: bright yellow
    Color(0xFF5C5CFF), // 12: bright blue
    Color(0xFFFF00FF), // 13: bright magenta
    Color(0xFF00FFFF), // 14: bright cyan
    Color(0xFFFFFFFF), // 15: bright white
  ];
}

/// A single terminal cell.
class Cell {
  final int ch; // Unicode code point
  final CellColor fg;
  final CellColor bg;
  final int flags;

  const Cell({
    required this.ch,
    required this.fg,
    required this.bg,
    required this.flags,
  });

  bool get isBold => (flags & CellFlags.bold) != 0;
  bool get isItalic => (flags & CellFlags.italic) != 0;
  bool get isUnderline => (flags & CellFlags.underline) != 0;
  bool get isDoubleUnderline => (flags & CellFlags.doubleUnderline) != 0;
  bool get isStrikethrough => (flags & CellFlags.strikethrough) != 0;
  bool get isHidden => (flags & CellFlags.hidden) != 0;
  bool get isDim => (flags & CellFlags.dim) != 0;
  bool get isInverse => (flags & CellFlags.inverse) != 0;
  bool get isWideChar => (flags & CellFlags.wideChar) != 0;
  bool get isWideCharSpacer => (flags & CellFlags.wideCharSpacer) != 0;
  bool get isDottedUnderline => (flags & CellFlags.dottedUnderline) != 0;
  bool get isDashedUnderline => (flags & CellFlags.dashedUnderline) != 0;

  /// Get the character as a string
  String get char {
    if (ch == 0) return ' ';
    return String.fromCharCode(ch);
  }

  /// Get effective foreground color (handles inverse)
  CellColor effectiveFg(bool inverse) {
    if (inverse) return bg;
    return fg;
  }

  /// Get effective background color (handles inverse)
  CellColor effectiveBg(bool inverse) {
    if (inverse) return fg;
    return bg;
  }
}
