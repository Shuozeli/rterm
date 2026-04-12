import 'dart:js_interop';
import 'package:web/web.dart' as web;
import 'models.dart';

/// Terminal renderer using HTML5 Canvas.
class TerminalRenderer {
  final web.HTMLCanvasElement _canvas;
  final web.CanvasRenderingContext2D _ctx;
  final int _cellWidth;
  final int _cellHeight;
  final String _fontFamily;

  int _cols = 80;
  int _rows = 24;
  ScreenBuffer? _screen;

  // Colors
  static const int _defaultFg = 0xFFE5E5E5;
  static const int _defaultBg = 0xFF000000;
  static const int _cursorColor = 0xFFFFFFFF;

  int get cellWidth => _cellWidth;
  int get cellHeight => _cellHeight;

  TerminalRenderer({
    required web.HTMLCanvasElement canvas,
    int cellWidth = 9,
    int cellHeight = 18,
    String fontFamily = 'monospace',
  })  : _canvas = canvas,
        _ctx = canvas.getContext('2d') as web.CanvasRenderingContext2D,
        _cellWidth = cellWidth,
        _cellHeight = cellHeight,
        _fontFamily = fontFamily {
    _setupCanvas();
  }

  void _setupCanvas() {
    _ctx.font = '${_cellHeight - 2}px $_fontFamily';
    _ctx.textBaseline = 'top';
  }

  void resize(int cols, int rows) {
    _cols = cols;
    _rows = rows;
    _canvas.width = cols * _cellWidth;
    _canvas.height = rows * _cellHeight;
    _setupCanvas();
  }

  void render(ScreenBuffer screen) {
    _screen = screen;
    _cols = screen.cols;
    _rows = screen.rows;
    print('[Render] ${screen.cols}x${screen.rows}, cursorVisible=${screen.cursorVisible}');

    // Use CSS display size as internal resolution to avoid browser scaling/distortion
    _canvas.width = _canvas.clientWidth;
    _canvas.height = _canvas.clientHeight;
    print('[Render] Canvas internal: ${_canvas.width}x${_canvas.height}');

    // Clear screen with default background
    _ctx.fillStyle = _colorToCss(_defaultBg).toJS;
    _ctx.fillRect(0, 0, _canvas.width, _canvas.height);

    // Draw cells
    for (int row = 0; row < screen.rows; row++) {
      for (int col = 0; col < screen.cols; col++) {
        final cell = screen.cells[row][col];
        _drawCell(cell, col, row);
      }
    }

    // Draw cursor
    if (screen.cursorVisible) {
      _drawCursor(screen.cursorRow, screen.cursorCol);
    }
  }

  void _drawCell(Cell cell, int col, int row) {
    // Debug: log first 3 rows and cols
    if (row < 3 && col < 5) {
      final cw = (_canvas.width / _cols).floor();
      final ch = (_canvas.height / _rows).floor();
      final x = col * cw;
      final y = row * ch;
      print('[Draw] row=$row col=$col x=$x y=$y baseline=${y + ch * 0.75} ch="${cell.ch}"');
    }

    // Skip empty cells with default background
    if (cell.ch == ' ' && cell.bg == _defaultBg) {
      return;
    }

    final x = col * (_canvas.width / _cols).floor();
    final y = row * (_canvas.height / _rows).floor();
    final cw = (_canvas.width / _cols).floor();
    final ch = (_canvas.height / _rows).floor();

    // Background - skip if COLOR_DEFAULT (0xFFFFFFFF) since canvas is already black
    if (cell.bg != 0xFFFFFFFF && cell.bg != _defaultBg) {
      _ctx.fillStyle = _colorToCss(cell.bg).toJS;
      _ctx.fillRect(x, y, cw, ch);
    }

    // Foreground
    final fg = cell.fg != 0 ? cell.fg : _defaultFg;
    _ctx.fillStyle = _colorToCss(fg).toJS;

    // Bold
    if (cell.bold) {
      _ctx.font = 'bold ${ch - 2}px $_fontFamily';
    } else {
      _ctx.font = '${ch - 2}px $_fontFamily';
    }

    // Underline
    if (cell.underline) {
      _ctx.fillStyle = _colorToCss(fg).toJS;
      _ctx.fillRect(x, y + ch - 3, cw, 2);
    }

    // Draw character - y is top of cell, fillText uses baseline so offset by ~75%
    if (cell.ch.isNotEmpty) {
      _ctx.fillText(cell.ch, x + 1, y + ch * 0.75);
    }
  }

  void _drawCursor(int row, int col) {
    final cw = (_canvas.width / _cols).floor();
    final ch = (_canvas.height / _rows).floor();
    final x = col * cw;
    final y = row * ch;

    // Cursor background (block cursor style)
    _ctx.fillStyle = _colorToCss(_cursorColor).toJS;
    _ctx.fillRect(x, y, cw - 1, ch - 1);

    // Invert the character at cursor position
    if (_screen != null) {
      final cell = _screen!.cells[row][col];
      final bg = cell.bg != 0xFF000000 ? cell.bg : _defaultBg;

      _ctx.fillStyle = _colorToCss(bg).toJS;
      _ctx.font = '${ch - 2}px $_fontFamily';
      _ctx.fillText(cell.ch, x + 1, y + ch * 0.75);
    }
  }

  // ANSI 256-color palette (indices 0-15 are standard colors)
  static const List<int> _ansiPalette = [
    0xFF000000, // 0: black
    0xFFCD0000, // 1: red
    0xFF00CD00, // 2: green
    0xFFCDCD00, // 3: yellow
    0xFF0000EE, // 4: blue
    0xFFCD00CD, // 5: magenta
    0xFF00CDCD, // 6: cyan
    0xFFE5E5E5, // 7: white
    0xFF7F7F7F, // 8: bright black (gray)
    0xFFFF0000, // 9: bright red
    0xFF00FF00, // 10: bright green
    0xFFFFFF00, // 11: bright yellow
    0xFF5C5CFF, // 12: bright blue
    0xFFFF00FF, // 13: bright magenta
    0xFF00FFFF, // 14: bright cyan
    0xFFFFFFFF, // 15: bright white
  ];

  String _colorToCss(int color) {
    // COLOR_DEFAULT = 0xFFFFFFFF means "use terminal default color"
    if (color == 0xFFFFFFFF) {
      // Return default foreground color
      return 'rgb(229,229,229)'; // light gray
    }

    // Check if this is an indexed color (format: 0xFF000000 | index)
    if ((color & 0xFF000000) == 0xFF000000) {
      final index = color & 0xFF;
      if (index < 16) {
        // Standard ANSI colors
        final c = _ansiPalette[index];
        final r = (c >> 16) & 0xFF;
        final g = (c >> 8) & 0xFF;
        final b = c & 0xFF;
        return 'rgb($r,$g,$b)';
      } else if (index < 232) {
        // 216-color RGB cube (16-231)
        final i = index - 16;
        final r = ((i ~/ 36) % 6) * 51;
        final g = ((i ~/ 6) % 6) * 51;
        final b = (i % 6) * 51;
        return 'rgb($r,$g,$b)';
      } else {
        // Grayscale ramp (232-255)
        final gray = (index - 232) * 10 + 8;
        return 'rgb($gray,$gray,$gray)';
      }
    }

    // RGB color (no alpha in high byte)
    final r = (color >> 16) & 0xFF;
    final g = (color >> 8) & 0xFF;
    final b = color & 0xFF;
    return 'rgb($r,$g,$b)';
  }

  /// Handle key event and return bytes to send.
  List<int>? handleKeyEvent(web.KeyboardEvent event) {
    final key = event.key;

    // Function keys
    switch (key) {
      case 'Enter':
        return [13];
      case 'Backspace':
        return [127];
      case 'Tab':
        return [9];
      case 'Escape':
        return [27];
      case 'ArrowUp':
        return [27, 91, 65];
      case 'ArrowDown':
        return [27, 91, 66];
      case 'ArrowRight':
        return [27, 91, 67];
      case 'ArrowLeft':
        return [27, 91, 68];
      case 'Home':
        return [27, 91, 72];
      case 'End':
        return [27, 91, 70];
      case 'Delete':
        return [27, 91, 51, 126];
      case 'F1':
        return [27, 79, 80];
      case 'F2':
        return [27, 79, 81];
      case 'F3':
        return [27, 79, 82];
      case 'F4':
        return [27, 79, 83];
      case 'F5':
        return [27, 91, 49, 53, 126];
      case 'F6':
        return [27, 91, 49, 55, 126];
      case 'F7':
        return [27, 91, 49, 56, 126];
      case 'F8':
        return [27, 91, 49, 57, 126];
      case 'F9':
        return [27, 91, 50, 48, 126];
      case 'F10':
        return [27, 91, 50, 49, 126];
      case 'F11':
        return [27, 91, 50, 51, 126];
      case 'F12':
        return [27, 91, 50, 52, 126];
    }

    // Ctrl combinations
    if (event.ctrlKey && key.length == 1) {
      final char = key.toLowerCase().codeUnitAt(0);
      if (char >= 97 && char <= 122) {
        return [char - 96]; // Ctrl+A = 1, etc.
      }
    }

    // Regular character
    if (key.length == 1 && !event.ctrlKey && !event.altKey && !event.metaKey) {
      return key.codeUnits;
    }

    return null;
  }
}
