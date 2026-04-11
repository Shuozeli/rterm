import 'cell.dart';

/// Cursor state from the server.
class CursorState {
  final int row;
  final int col;
  final bool visible;
  final int style;

  const CursorState({
    required this.row,
    required this.col,
    required this.visible,
    required this.style,
  });
}

/// A row of cells with metadata.
class CellRow {
  final int rowIndex;
  final List<Cell> cells;

  CellRow({required this.rowIndex, required this.cells});

  int get length => cells.length;
  Cell operator [](int col) => cells[col];
}

/// Screen buffer holding all terminal cells.
class ScreenBuffer {
  int cols;
  int rows;
  int viewportOffset; // History lines above visible screen
  List<CellRow> buffer;
  CursorState cursor;
  String title;
  bool altScreenActive;
  bool applicationCursorKeys;

  ScreenBuffer({
    required this.cols,
    required this.rows,
    this.viewportOffset = 0,
    required this.buffer,
    required this.cursor,
    this.title = '',
    this.altScreenActive = false,
    this.applicationCursorKeys = false,
  });

  /// Create an empty screen buffer
  factory ScreenBuffer.empty(int cols, int rows) {
    final emptyCell = const Cell(
      ch: 0,
      fg: CellColor(CellColor.defaultColor),
      bg: CellColor(CellColor.defaultColor),
      flags: 0,
    );
    final buffer = List.generate(
      rows,
      (i) => CellRow(
        rowIndex: i,
        cells: List.filled(cols, emptyCell),
      ),
    );
    return ScreenBuffer(
      cols: cols,
      rows: rows,
      buffer: buffer,
      cursor: const CursorState(row: 0, col: 0, visible: true, style: 0),
    );
  }

  /// Get cell at position (row, col)
  Cell cellAt(int row, int col) {
    if (row < 0 || row >= rows || col < 0 || col >= cols) {
      return const Cell(
        ch: 0,
        fg: CellColor(CellColor.defaultColor),
        bg: CellColor(CellColor.defaultColor),
        flags: 0,
      );
    }
    return buffer[row][col];
  }

  /// Check if position is within bounds
  bool inBounds(int row, int col) {
    return row >= 0 && row < rows && col >= 0 && col < cols;
  }
}
