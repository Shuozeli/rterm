/// Cell data representing a single terminal cell.
class Cell {
  final String ch;
  final int fg;
  final int bg;
  final int flags;

  const Cell({
    this.ch = ' ',
    this.fg = 0xFFFFFFFF,
    this.bg = 0xFF000000,
    this.flags = 0,
  });

  bool get bold => (flags & 1) != 0;
  bool get italic => (flags & 2) != 0;
  bool get underline => (flags & 4) != 0;
  bool get strikethrough => (flags & 8) != 0;
  bool get hidden => (flags & 16) != 0;

  Cell copyWith({String? ch, int? fg, int? bg, int? flags}) {
    return Cell(
      ch: ch ?? this.ch,
      fg: fg ?? this.fg,
      bg: bg ?? this.bg,
      flags: flags ?? this.flags,
    );
  }
}

/// Screen buffer holding terminal cells.
class ScreenBuffer {
  int cols;
  int rows;
  List<List<Cell>> cells;
  int cursorRow = 0;
  int cursorCol = 0;
  bool cursorVisible = true;
  bool altScreenActive = false;

  ScreenBuffer({required this.cols, required this.rows})
      : cells = List.generate(
          rows,
          (_) => List.generate(cols, (_) => const Cell()),
        );

  factory ScreenBuffer.empty(int cols, int rows) {
    return ScreenBuffer(cols: cols, rows: rows);
  }

  void setCell(int row, int col, Cell cell) {
    if (row >= 0 && row < rows && col >= 0 && col < cols) {
      cells[row][col] = cell;
    }
  }

  void resize(int newCols, int newRows) {
    if (newCols == cols && newRows == rows) return;

    final newCells = List.generate(
      newRows,
      (_) => List.generate(newCols, (_) => const Cell()),
    );

    // Copy existing data
    for (int r = 0; r < rows && r < newRows; r++) {
      for (int c = 0; c < cols && c < newCols; c++) {
        newCells[r][c] = cells[r][c];
      }
    }

    cols = newCols;
    rows = newRows;
    cells = newCells;
  }
}

/// Screen update from server.
class ScreenUpdate {
  final int x;
  final int y;
  final int width;
  final int height;
  final List<Cell> cells;

  ScreenUpdate({
    required this.x,
    required this.y,
    required this.width,
    required this.height,
    required this.cells,
  });
}

/// Host profile for connections.
class HostProfile {
  final String id;
  final String name;
  final String hostname;
  final int port;
  final String username;
  final String? password;
  final String? relayUrl;

  HostProfile({
    required this.id,
    required this.name,
    required this.hostname,
    this.port = 22,
    required this.username,
    this.password,
    this.relayUrl,
  });
}

/// Color utilities.
class Colors {
  static const List<int> palette = [
    0xFF000000, // 0: black
    0xFFCD0000, // 1: red
    0xFF00CD00, // 2: green
    0xFFCDCD00, // 3: yellow
    0xFF0000EE, // 4: blue
    0xFFCD00CD, // 5: magenta
    0xFF00CDCD, // 6: cyan
    0xFFE5E5E5, // 7: white
    0xFF7F7F7F, // 8: bright black
    0xFFFF0000, // 9: bright red
    0xFF00FF00, // 10: bright green
    0xFFFFFF00, // 11: bright yellow
    0xFF5C5CFF, // 12: bright blue
    0xFFFF00FF, // 13: bright magenta
    0xFF00FFFF, // 14: bright cyan
    0xFFFFFFFF, // 15: bright white
  ];

  static int ansi(int code) {
    if (code < 16) return palette[code];
    if (code < 232) {
      final i = code - 232;
      final r = (i ~/ 36) * 51;
      final g = ((i ~/ 6) % 6) * 51;
      final b = (i % 6) * 51;
      return 0xFF000000 | (r << 16) | (g << 8) | b;
    }
    final gray = ((code - 232) * 10 + 8) & 0xFF;
    return 0xFF000000 | (gray << 16) | (gray << 8) | gray;
  }
}
